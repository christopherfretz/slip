//! The keys a transform's declarations need (`docs/wp/WP-087.md`,
//! `spec/api.md` §7.5, `spec/verify.md` §2.1, §9.5, `spec/edits.md` §9.7).
//!
//! [`Keys`] answers, for one translation unit, what the host of a
//! transform (and the native rename) must name by key: the entity of a
//! declaration, the unit that holds it, its key and key id, the units
//! derived from it and the rule that undoes a rename. Every answer comes
//! from the functions the walk itself calls (`Walk::entity`,
//! `Walk::key`, `Walk::key_bytes`, `units::unit_class`,
//! `local::unit_of`), on a `Walk` of its own, or from the walk itself:
//! `derived` lists units the walk writes, and runs it once to know them.
//! Nothing here derives a key's field a second way, and nothing here is
//! called by the walk, so no digest depends on this module.
//!
//! # What it does not see
//!
//! - **Collisions** (digest §3.4) are found by the walk over all units;
//!   a key that collides in the translation unit is given all the same.
//!   Its unit is indeterminate, so a declaration about it exempts or
//!   renames nothing that could compare equal.
//! - **Taints** (`units::apply_taints`): a unit made indeterminate by a
//!   declaration of a kind without a key kind keeps its key here.
//!
//! # The stack
//!
//! A method may run a key computation, a unit's stream (`derived`) or a
//! numbering of an owner (`local.rs`), whose recursion `lib.rs` "What
//! bounds the stack" measures: a caller runs `Keys` on a thread of
//! [`crate::STACK_SIZE`], as it runs `digest`. A numbering is computed
//! when first needed (the memo is never sealed here), so it may run on
//! top of a unit's stream in `derived`: the stack then holds two walks'
//! budgets, which `STACK_SIZE` has room for (it is more than four).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::ids::DeclId;
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, KeyKind, Name, Rule, RuleTable};

use crate::keys::{KeyRes, Role, info};
use crate::walk::Walk;
use crate::{Cause, Input};

/// Why [`Keys`] gives no key, or no rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoKey {
    /// Not an entity the digest keys: a parameter, a local object, a
    /// declaration of a kind without a key kind (a linkage
    /// specification, an access specifier).
    NotAUnit,
    /// A unit keyed by content (digest §3.3): its key is its digest, so
    /// no declaration can name it across a change (`spec/api.md` §7.5
    /// rule 2, `spec/verify.md` §2.2).
    ContentKeyed,
    /// The key cannot be computed, or carries a cause (a discriminator
    /// with a pending area): the unit is indeterminate (digest §10), and
    /// a key it has may not tell it from another entity.
    Indeterminate(Cause),
    /// `rename_rule` only (`spec/api.md` §7.5 rule 3, `unsupported`): the
    /// entity has a key, but it is no unit keyed by name: its name is no
    /// identifier (an operator, a constructor, an unnamed class, the
    /// unnamed namespace), or its key only follows another entity's (a
    /// specialisation, a partial specialisation, a decomposition:
    /// `spec/landed/digest/WP-087.md`).
    NotRenamable,
    /// `rename_rule` only (`spec/api.md` §7.5 rule 3, `invalid_argument`):
    /// the new name is no identifier as this module reads one
    /// (`[A-Za-z_][A-Za-z0-9_]*`; a keyword passes, a universal character
    /// name or a code point outside ASCII does not: limits of
    /// `spec/landed/digest/WP-087.md`), or it is the entity's own name.
    BadName,
}

/// The cause `Walk::key` gives a local entity that is not a local type
/// (`keys.rs` `LOCAL_ENTITY`): it has no key, which is [`NoKey::NotAUnit`]
/// here. Matched by value; `tests/api_keys.rs` (a local variable) fails
/// if `keys.rs` renames it.
const LOCAL_ENTITY: Cause = Cause::Unaccounted("key/local");

/// The cause of a key whose bytes, computed again, are not the key the
/// walk memoised: an internal defect, never met (test 8).
const KEY_AGAIN: Cause = Cause::Unaccounted("query/key-again");

/// How far a chain of contexts is followed.
const CTX_LIMIT: u32 = 4096;

/// The rule table of [`Keys::new`]: none.
static NO_RULES: LazyLock<RuleTable> = LazyLock::new(RuleTable::empty);

/// The keys of one translation unit, computed by the walk's functions on
/// demand and memoised (the walk's own memo of keys, and the units
/// [`Keys::unit_of`] found).
pub struct Keys<'a> {
    input: &'a Input<'a>,
    rules: &'a RuleTable,
    read_only: crate::ReadOnly<'a>,
    w: Walk<'a>,
    units: HashMap<DeclId, Option<DeclId>>,
    /// The edges [`Keys::derived`] follows, built on first use.
    origins: Option<HashMap<DeclId, Vec<DeclId>>>,
    /// The units the walk writes, by entity, run on first use of
    /// [`Keys::derived`].
    written: Option<HashMap<DeclId, Vec<WalkUnit>>>,
}

impl std::fmt::Debug for Keys<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys").finish_non_exhaustive()
    }
}

impl<'a> Keys<'a> {
    /// The keys of `input`, with no rules: the pre-image's key space.
    /// Runs nothing.
    pub fn new(input: &'a Input<'a>) -> Keys<'a> {
        Keys::with_rules(input, &NO_RULES, crate::ReadOnly::EVERY_FILE)
    }

    /// The same over a post-image, with the Slip run's rules: the keys
    /// are then mapped, in the pre-image's key space (`spec/verify.md`
    /// §2.1). Runs nothing.
    ///
    /// `read_only` is the run's read-only files, as `Options::read_only`
    /// (`spec/verify.md` §2.1, "Rules rename owned entities only"): a
    /// rule renames owned entities only, so these keys are the verify
    /// job's only when given the job's files. `ReadOnly::NO_FILE` maps as
    /// "every file owned"; `ReadOnly::EVERY_FILE` maps nothing.
    ///
    /// A table that holds a rule §2.1 refuses (`RuleTable::refused`) is
    /// taken as the walk takes it: nothing is keyed under it. Every
    /// `key_id`, `key` and `rename_rule` of an entity with a key kind is
    /// `Indeterminate` with the walk's cause, and `derived` is empty.
    pub fn with_rules(
        input: &'a Input<'a>,
        rules: &'a RuleTable,
        read_only: crate::ReadOnly<'a>,
    ) -> Keys<'a> {
        let mut w = Walk::new(input, rules);
        w.read_only = read_only;
        Keys { input, rules, read_only, w, units: HashMap::new(), origins: None, written: None }
    }

    /// The declaration the digest treats as the entity of `d`
    /// (`Walk::entity`): its first declaration, and for the pattern of a
    /// template the template.
    pub fn entity(&self, d: DeclId) -> DeclId {
        self.w.entity(d)
    }

    /// The entity that is the digest unit holding the declaration `d`
    /// (digest §2.1): `d`'s entity when it is a unit, else the innermost
    /// enclosing entity that is. A local entity's is `local::unit_of`'s
    /// (its owner, or the owner's own unit when the owner is written in
    /// place); a template parameter's is its template's. `None` for the
    /// translation unit, a lifetime-extended temporary, and a declaration
    /// no unit holds.
    ///
    /// The unit is the one whose stream holds the declaration, not the
    /// one whose bytes may hold what it says: a closure in the default
    /// member initialiser of a field of an implicit instantiation is
    /// held by the field, and its bytes are in the field's
    /// `<default_argument 0>` unit (digest-kinds §11.4), which
    /// [`Keys::derived`] of the field lists. An implicit member nothing
    /// uses is its own unit, though the walk never writes it
    /// (`spec/landed/digest/WP-087.md`).
    pub fn unit_of(&mut self, d: DeclId) -> Option<DeclId> {
        if let Some(&u) = self.units.get(&d) {
            return u;
        }
        let u = self.find_unit(d);
        self.units.insert(d, u);
        u
    }

    fn find_unit(&mut self, d: DeclId) -> Option<DeclId> {
        let w = &mut self.w;
        let ast = w.ast;
        let mut x = w.entity(d);
        for _ in 0..CTX_LIMIT {
            let kind = ast.decls.get(x).kind();
            match info(kind).role {
                Role::Root => return None,
                Role::InParent => {
                    // A template parameter: its template's unit holds it
                    // (Kiln gives it the template's context, which does
                    // not). Any other (a lifetime-extended temporary):
                    // no unit is said to hold it.
                    if !matches!(
                        kind,
                        DeclKind::TemplateTypeParm
                            | DeclKind::NonTypeTemplateParm
                            | DeclKind::TemplateTemplateParm
                    ) {
                        return None;
                    }
                    crate::local::param_templates(w);
                    x = w.entity(crate::local::param_template(w, x)?);
                    continue;
                }
                Role::Entity(_) | Role::Content(_) => {
                    if crate::units::unit_class(w, x).is_some() {
                        return Some(x);
                    }
                }
                Role::Nothing | Role::Transparent | Role::Local | Role::Unsupported(_) => {}
            }
            if w.is_local(x)
                && !w.is_local_extern(x)
                && let Some(u) = crate::local::unit_of(w, x)
            {
                return Some(u);
            }
            x = w.entity(ast.decls.get(x).ctx()?);
        }
        None
    }

    /// The key id of the unit `e` (of `e`'s entity), mapped under this
    /// `Keys`' rules: in the pre-image's key space. Also given for an
    /// entity that has a key and is no unit (a local type, a member of a
    /// local class, a specialisation only named), whose key a stream
    /// writes.
    pub fn key_id(&mut self, e: DeclId) -> Result<Digest, NoKey> {
        let e = self.w.entity(e);
        // Under a table that holds a refused rule (`RuleTable::refused`)
        // the walk keys nothing, a content-keyed unit included: its
        // refusal is `Walk::key`'s, with the walk's cause.
        if let Role::Content(_) = info(self.w.ast.decls.get(e).kind()).role
            && self.rules.refused().is_none()
        {
            return Err(NoKey::ContentKeyed);
        }
        match self.w.key(e) {
            KeyRes::Key { id, cause: None, .. } => Ok(id),
            KeyRes::None(None) => Err(NoKey::NotAUnit),
            KeyRes::None(Some(c)) if c == LOCAL_ENTITY => Err(NoKey::NotAUnit),
            KeyRes::Key { cause: Some(c), .. } | KeyRes::None(Some(c)) => {
                Err(NoKey::Indeterminate(c))
            }
        }
    }

    /// Whether the rules may give a namespace a second key
    /// (`ns_refs.rs`): without, every namespace's key is `key_id`'s.
    pub(crate) fn ns_rules(&self) -> bool {
        self.w.ns_rules()
    }

    /// A5 (WP-141): the single key R1 writes for namespace `ns` named to
    /// find `found` (`None`: `Ref::views`, or no key).
    pub(crate) fn ns_found_key(&mut self, ns: DeclId, found: &[DeclId]) -> Option<Digest> {
        self.w.ns_found_key(ns, found)
    }

    /// A5: the view keys of namespace `ns`.
    pub(crate) fn ns_view_keys(&mut self, ns: DeclId) -> Vec<Digest> {
        self.w.ns_view_keys(ns).into_iter().map(|(_, k)| k).collect()
    }

    /// A5: the key of the whole reading `d` (a using-directive or alias)
    /// writes for its target (`None`: not whole).
    pub(crate) fn ns_whole_key(&mut self, d: DeclId) -> Option<Digest> {
        self.w.ns_whole_key(d)
    }

    /// The key itself (digest §3): `key(e)?.id() == key_id(e)?`, and the
    /// two refuse alike. Under `with_rules` it is the mapped key. Its
    /// `parent` is a key id: the key of the enclosing entity is `key` of
    /// that entity.
    pub fn key(&mut self, e: DeclId) -> Result<Key, NoKey> {
        let id = self.key_id(e)?;
        let key = self.w.key_bytes(e).and_then(|b| Key::from_bytes(&b).ok());
        match key {
            Some(k) if k.id() == id => Ok(k),
            Some(_) | None => {
                debug_assert!(false, "the key of {e:?} computed again is not the walk's");
                Err(NoKey::Indeterminate(KEY_AGAIN))
            }
        }
    }

    /// The key ids of the units of this translation unit that digest
    /// §2.2 derives from `e`, sorted, without duplicates: the resolved
    /// and `default_argument` units of `e`, and the units of the derived
    /// groups whose entity is reached from `e` through an instantiation
    /// (of `e`, of its members at any depth of classes, of the local
    /// types it owns, of their members), with their resolved and
    /// `default_argument` units, and every unit of such an entity that is
    /// explicitly instantiated (written in source, so its group is a
    /// declared one). Only units the walk writes are listed:
    /// an on-demand unit is listed when, and only when, this translation
    /// unit refers to it.
    ///
    /// The first call builds an index of every declaration's origins and
    /// runs the walk over the translation unit, as `digest` does, to know
    /// which units it writes.
    pub fn derived(&mut self, e: DeclId) -> Vec<Digest> {
        // Under a refused table no unit has a key to list.
        if self.rules.refused().is_some() {
            return Vec::new();
        }
        let e = self.w.entity(e);
        let mut found: Vec<DeclId> = Vec::new();
        {
            let origins = self.origins.get_or_insert_with(|| build_origins(&self.w));
            let mut seen: HashSet<DeclId> = HashSet::new();
            let mut stack = vec![e];
            seen.insert(e);
            while let Some(x) = stack.pop() {
                for &y in origins.get(&x).map_or(&[][..], Vec::as_slice) {
                    if seen.insert(y) {
                        stack.push(y);
                        found.push(y);
                    }
                }
            }
        }
        let (input, rules, read_only) = (self.input, self.rules, self.read_only);
        let ast = self.w.ast;
        let written = self.written.get_or_insert_with(|| {
            let mut by: HashMap<DeclId, Vec<WalkUnit>> = HashMap::new();
            for u in walk_units(input, rules, read_only) {
                by.entry(u.entity).or_default().push(u);
            }
            by
        });
        let mut out: Vec<Digest> = Vec::new();
        let of = |x: DeclId| written.get(&x).map_or(&[][..], Vec::as_slice);
        out.extend(of(e).iter().filter(|u| u.part).map(|u| u.key));
        for y in found {
            // An explicit instantiation (`template int t<long>(long);`,
            // `extern template`, a member of an explicitly instantiated
            // class) is written in source, so the walk groups its units as
            // declared; it is still instantiated from `e` and changes with
            // it (`spec/landed/api/WP-095.md`, F3).
            let explicit = ast.governing_explicit_inst_kind(y).is_some();
            out.extend(of(y).iter().filter(|u| u.derived || explicit).map(|u| u.key));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// `spec/verify.md` §2.1: the rule under which a post-image where `e`
    /// is declared `new_name` in the same scope has `e`'s key. Read from
    /// [`Keys::key`]: the rule's parents are both that key's parent (the
    /// same scope, in mapped terms; `c_linkage` for an entity with C
    /// language linkage, verify §2.3), its kind and discriminator the
    /// key's, its name `identifier(new_name)` and the name it is to be
    /// called by the key's.
    ///
    /// `new_name` must be an identifier as `slip declare` reads one
    /// (`[A-Za-z_][A-Za-z0-9_]*`, verify §2.3) and not the name `e` has.
    pub fn rename_rule(&mut self, e: DeclId, new_name: &[u8]) -> Result<Rule, NoKey> {
        // `invalid_argument` is judged first, as §7.5 rule 3 lists it,
        // from the text alone.
        if !is_identifier(new_name) {
            return Err(NoKey::BadName);
        }
        let k = self.key(e)?;
        match k.kind {
            KeyKind::Specialisation | KeyKind::PartialSpecialisation | KeyKind::Decomposition => {
                return Err(NoKey::NotRenamable);
            }
            _ => {}
        }
        let Name::Identifier(old) = &k.name else { return Err(NoKey::NotRenamable) };
        if old.as_slice() == new_name {
            return Err(NoKey::BadName);
        }
        Ok(Rule {
            parent: k.parent,
            kind: k.kind,
            name: Name::Identifier(new_name.to_vec()),
            disc: k.disc,
            to_parent: k.parent,
            to_name: k.name,
        })
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
fn is_identifier(n: &[u8]) -> bool {
    match n.split_first() {
        Some((&first, rest)) => {
            (first.is_ascii_alphabetic() || first == b'_')
                && rest.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_')
        }
        None => false,
    }
}

/// For every entity, the entities whose origin it is: a specialisation
/// from its template (and from the pattern it was instantiated from), a
/// member of a specialisation from the member it was instantiated from,
/// anything Kiln records an `instantiated_from` for from that; a member
/// from its class, semantic and lexical; a local type from its owner;
/// the holding variable of a tuple-like binding from the binding; and
/// the unnamed object of an anonymous class from the class.
fn build_origins(w: &Walk<'_>) -> HashMap<DeclId, Vec<DeclId>> {
    let ast = w.ast;
    let mut edges: HashMap<DeclId, Vec<DeclId>> = HashMap::new();
    let mut add = |from: DeclId, to: DeclId| {
        let from = w.entity(from);
        if from != to {
            let v = edges.entry(from).or_default();
            if !v.contains(&to) {
                v.push(to);
            }
        }
    };
    for d in ast.decls.ids() {
        let x = w.entity(d);
        if let Some((_, s)) = ast.spec_info(d) {
            add(s.template, x);
            add(s.from, x);
        }
        if let Some((_, m)) = ast.member_spec_info(d) {
            add(m.instantiated_from, x);
        }
        if let Some(p) = ast.templates.instantiated_from_map(d) {
            add(p, x);
        }
        let n = ast.decls.get(d);
        for c in [n.ctx(), ast.decls.lexical_ctx(d)].into_iter().flatten() {
            if ast.decls.record(c).is_some() {
                add(c, x);
            }
        }
        if crate::local::is_local_type(w, d)
            && let Some(o) = crate::local::owner(w, d)
        {
            add(o, x);
        }
        if let Some(&h) = ast.decls.cold.binding_var.get(d) {
            add(d, w.entity(h));
        }
        if matches!(n.kind(), DeclKind::Var | DeclKind::Field)
            && !matches!(ast.names.view(n.name()), DeclNameView::Identifier(s) if !ast.symbols().as_str(s).is_empty())
            && let Some(r) = n.ty().and_then(|q| ast.types.as_record(ast.types.canonical(q)))
        {
            add(r, x);
        }
    }
    edges
}

/// One unit the walk writes (the order unit left out), as
/// [`walk_units`] gives it.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkUnit {
    /// The entity (for a resolved or `default_argument` unit, its owner).
    pub entity: DeclId,
    pub key: Digest,
    /// In a derived group (digest §2.2).
    pub derived: bool,
    /// A resolved or `default_argument` unit.
    pub part: bool,
    /// For an entity's own unit, the cause the walk's `Walk::key` gives
    /// its key (`KeyRes::Key { cause }`), read from the walk's own memo
    /// once it has written every unit.
    pub key_cause: Option<Cause>,
}

/// Every unit of the walk of `input` under `rules` but the order unit,
/// in the order the walk wrote them. The oracle of `tests/api_keys.rs`
/// (tests 1 and 3), and what [`Keys::derived`] lists from: the walk's
/// `Written` records, which are not public otherwise. Runs the whole
/// walk; changes no digest.
#[doc(hidden)]
pub fn walk_units(
    input: &Input<'_>,
    rules: &RuleTable,
    read_only: crate::ReadOnly<'_>,
) -> Vec<WalkUnit> {
    let options = crate::Options {
        rules,
        read_only,
        detail: crate::Detail::None,
        names: false,
        rule_use: false,
    };
    crate::walk::run_with(input, &options, |w, written, _| {
        written
            .iter()
            .filter_map(|u| {
                let entity = u.entity?;
                let part = u.resolved.is_some() || u.default_argument.is_some() || u.operands.is_some();
                let key_cause = match (part, w.key(entity)) {
                    (false, KeyRes::Key { cause, .. }) => cause,
                    _ => None,
                };
                Some(WalkUnit {
                    entity,
                    key: u.key,
                    derived: u.class == GroupClass::Derived,
                    part,
                    key_cause,
                })
            })
            .collect()
    })
}
