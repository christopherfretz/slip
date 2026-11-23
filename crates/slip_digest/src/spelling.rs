//! Spelled entities in one observer (`spec/verify.md` §10.2, §10.6;
//! `docs/wp/WP-096.md`; what the spec leaves open is decided in
//! `spec/landed/verify/WP-096.md`).
//!
//! A spelled-entity piece says which entity a name is to denote and
//! leaves the text to Slip (`spec/api.md` §7.2); the native rename writes
//! one at every resolved reference it renames (`spec/edits.md` §9.3).
//! Where the piece's text landed in the post-image is an obligation
//! (§10.1). [`check_spellings`] answers, for one parse of the post-image,
//! whether the name there denotes the entity the obligation records.
//!
//! This is the trusted base. A wrong [`Spelling::Holds`] lets a broken
//! refactor through as verified; a wrong `Fails` or `NotSeen` is a false
//! alarm. So every doubt is `NotSeen` or `Fails`:
//!
//! - **Nothing is computed a second way.** The written references are
//!   `slip_kiln::written::written_refs`, the name range of one is
//!   `slip_kiln::ranges::file_range_of_span` (Kiln's
//!   `editable_file_range`), the origin of a referent is
//!   `slip_kiln::written::origin`, and the key of an origin is
//!   [`Keys::key_id`] under the run's rules ([`Keys::with_rules`]), so
//!   it is the key the walk of the same parse gives the entity. Whether a
//!   key collides (digest §3.4) is the walk's own marking
//!   (`units::mark_collisions`, `keys::mark_mapped_collisions`), run on
//!   the walk's own units.
//! - **Every recorded name takes part, whatever its role**, and a name
//!   Kiln records without a referent (dependent, or whose resolution
//!   failed) answers and fails: the question is what the name denotes,
//!   and an answer covers the names Kiln records at the range (§10.2
//!   rules 1 and 5; what it does not record is §10.7's named hole).
//! - **A reference whose name has no range takes no part**: Kiln gives
//!   none, `ranges` refuses one (an empty span, a directive line, a
//!   `<scratch space>` buffer), the file's text is not its bytes (Kiln
//!   replaced invalid UTF-8, `spec/edits.md` §2.3 `file_recoded`), or the
//!   buffer has no subject path.
//! - **A key that is not one entity's is no key**: a key that collides
//!   in the unit is treated as an indeterminate key, and fails.

use std::collections::{HashMap, HashSet};

use slip_kiln::Unit;
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::source::{FileId, SourceManager, Span};
use slip_kiln::ranges;
use slip_kiln::written::{self, Role};
use slip_model::Digest;
use slip_model::keys::{KeyKind, RuleTable};

use crate::query::Keys;
use crate::{Cause, Detail, Input, Options};

/// One obligation of `spec/verify.md` §10.1 as the check takes it: the
/// file by its subject path (digest §11), the bytes `[lo, hi)` of its
/// post-image where the text of one spelled piece landed, and the key id
/// the piece carries (in the pre-image's key space).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obligation {
    /// The subject path of the file.
    pub file: Vec<u8>,
    pub lo: u32,
    pub hi: u32,
    pub key: Digest,
}

/// What one observer says about one obligation (`spec/verify.md` §10.2
/// rule 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spelling {
    /// At least one reference answers, and every answering reference's
    /// origin has the obligation's key.
    Holds,
    /// No reference answers.
    NotSeen,
    /// Some answering reference's origin has another key, or none, or
    /// some answering name has no referent.
    Fails {
        /// The key, under the rules, of the origin of the first answering
        /// reference in unit order that does not have the obligation's
        /// key; `None` when it has none (it is not a unit, is keyed by
        /// content, its key is indeterminate or collides) or the name has
        /// no referent ([`UNRESOLVED_DEPENDENT`], [`UNRESOLVED_FAILED`]).
        found: Option<Digest>,
        /// That entity as a person would write it (`spec/verify.md`
        /// §6.1): for the report only.
        found_name: String,
    },
}

/// Checks the obligations whose file this unit read, in one parse of the
/// post-image (`spec/verify.md` §10.2). The result is parallel to
/// `obligations`.
///
/// `unit` and `input` are of the same parse: `input` is the digest's
/// view of `unit` (its `ast` and `sm` are `unit`'s) with the subject
/// paths of the run. If they are not (a caller's error), every answer is
/// [`Spelling::NotSeen`]. An obligation whose `file` is a path the unit
/// did not read, or whose `lo` is not below `hi`, is `NotSeen`: no
/// reference can answer it.
///
/// **The unit must have been parsed with
/// [`ParseOptions::written`](slip_kiln::ParseOptions::written)**, as the
/// verify job of a transform's run always does (`docs/wp/WP-074.md`).
/// Without the tables (`slip_kiln::written::recorded` is false) no
/// reference is known and every obligation is `NotSeen`, which over the
/// run is `unseen`: safe, with the wrong message.
///
/// Under a table [`RuleTable::refused`] refuses, no entity has a key
/// (`Keys::with_rules`): an obligation some reference answers fails with
/// `found` absent, one none answers is `NotSeen`. Never `Holds`.
///
/// Runs on a thread of [`crate::STACK_SIZE`], as [`crate::digest`] does:
/// it computes keys, and runs the walk once over the unit (to know which
/// keys collide) when some obligation would hold.
pub fn check_spellings(
    unit: &Unit<'_>,
    input: &Input<'_>,
    rules: &RuleTable,
    obligations: &[Obligation],
) -> Vec<Spelling> {
    let not_seen = || vec![Spelling::NotSeen; obligations.len()];
    if obligations.is_empty() {
        return Vec::new();
    }
    if !std::ptr::eq(unit.ast(), input.ast) || !std::ptr::eq(unit.sm(), input.sm) {
        debug_assert!(false, "check_spellings: the unit and the input are of two parses");
        return not_seen();
    }
    if !written::recorded(unit) {
        return not_seen();
    }
    let placed = placed_refs(unit, input);
    let mut check = Check {
        unit,
        input,
        keys: Keys::with_rules(input, rules),
        rules,
        origins: HashMap::new(),
        walk: None,
    };
    obligations.iter().map(|ob| check.one(&placed, ob)).collect()
}

/// `found_name` of a name Kiln records without a referent because it is
/// dependent (`REF_DEPENDENT`): what it denotes is decided per
/// instantiation, and Kiln does not say per instantiation.
pub const UNRESOLVED_DEPENDENT: &str = "<unresolved name: dependent>";

/// `found_name` of a name Kiln records without a referent because its
/// lookup or overload resolution failed (`REF_UNRESOLVED`).
pub const UNRESOLVED_FAILED: &str = "<unresolved name: resolution failed>";

/// A recorded name that takes part (§10.2 rules 1 and 2), with its range.
struct Placed {
    /// The subject path of the range's file.
    path: Vec<u8>,
    lo: u32,
    hi: u32,
    /// The referent; `None` for a name without one.
    referent: Option<DeclId>,
    /// For a name without a referent: resolution failed, rather than
    /// waiting for an instantiation.
    unresolved: bool,
}

/// Whether a name of role `role` takes part (§10.2 rule 1): every role
/// does, because the check asks what the name denotes, whatever the
/// syntax around it (`spec/api.md` §6.5's roles are what a transform is
/// given, another question). Every role is listed, so that a role Kiln
/// adds does not compile until someone has looked at it here.
fn takes_part(role: Role) -> bool {
    match role {
        Role::Expr
        | Role::Member
        | Role::Nns
        | Role::Type
        | Role::Template
        | Role::Base
        | Role::MemInit
        | Role::UsingDecl
        | Role::UsingDirective
        | Role::NamespaceAlias
        | Role::Elaborated
        | Role::SpecTarget
        | Role::Concept
        | Role::Designator => true,
    }
}

/// The recorded names of the unit that take part, with their ranges, in
/// unit order (`slip_kiln::ranges::unit_order` of their starts; among
/// equal ones the references in the table's order, then the names
/// without a referent in the table's order).
fn placed_refs(unit: &Unit<'_>, input: &Input<'_>) -> Vec<Placed> {
    let sm = unit.sm();
    let mut names: Vec<(Span, Option<DeclId>, bool)> = written::written_refs(unit)
        .into_iter()
        .filter(|r| takes_part(r.role))
        .map(|r| (r.name, Some(r.referent), false))
        .collect();
    names.extend(
        written::dependent_names(unit)
            .into_iter()
            .filter(|d| takes_part(d.role))
            .map(|d| (d.name, None, d.unresolved)),
    );
    names.sort_by(|a, b| ranges::unit_order(sm, a.0.lo, b.0.lo));
    let mut paths: HashMap<FileId, Vec<u8>> = HashMap::new();
    let mut out = Vec::new();
    for (name, referent, unresolved) in names {
        let Some(fr) = ranges::file_range_of_span(sm, name) else { continue };
        if !text_is_bytes(sm, fr.file) {
            continue;
        }
        let path = paths.entry(fr.file).or_insert_with(|| (input.subject_path)(fr.file));
        if path.is_empty() {
            continue;
        }
        out.push(Placed { path: path.clone(), lo: fr.lo, hi: fr.hi, referent, unresolved });
    }
    out
}

/// Whether Kiln's text of `file` is its bytes, so that offsets in one are
/// offsets in the other (`spec/edits.md` §2.3, `file_recoded`).
fn text_is_bytes(sm: &SourceManager, file: FileId) -> bool {
    sm.load_report(file).is_clean()
}

/// The keys that collide in this unit (digest §3.4), as the walk marks
/// them.
struct Collisions {
    /// Keys of units the walk marks `Collision`.
    keys: HashSet<Digest>,
    /// Keys of units the walk writes (but the order unit).
    with_unit: HashSet<Digest>,
    /// The walk marked the order unit: some key that no unit has collides
    /// (`keys::mark_mapped_collisions`), which one is not said.
    unitless: bool,
    /// The names of the walk's units, by key (`spec/verify.md` §6.1).
    names: HashMap<Digest, String>,
}

impl Collisions {
    /// Whether `k` may be the key of two entities.
    fn collides(&self, k: &Digest) -> bool {
        self.keys.contains(k) || (self.unitless && !self.with_unit.contains(k))
    }
}

struct Check<'a, 'u> {
    unit: &'a Unit<'u>,
    input: &'a Input<'a>,
    keys: Keys<'a>,
    rules: &'a RuleTable,
    /// The key of each origin asked for, memoised: `None` when it has
    /// none or its key is not one entity's.
    origins: HashMap<DeclId, Option<Digest>>,
    /// The walk's collisions and names, made on first need.
    walk: Option<Collisions>,
}

impl Check<'_, '_> {
    fn one(&mut self, placed: &[Placed], ob: &Obligation) -> Spelling {
        if ob.lo >= ob.hi || ob.file.is_empty() {
            return Spelling::NotSeen;
        }
        // §10.2 rule 3: in the file, ending at `hi`, starting at or after
        // `lo`; in unit order.
        let answering: Vec<&Placed> =
            placed.iter().filter(|p| p.path == ob.file && p.hi == ob.hi && p.lo >= ob.lo).collect();
        if answering.is_empty() {
            return Spelling::NotSeen;
        }
        // §10.2 rules 4 and 5: every answering name has a referent whose
        // origin has the key; the first that does not is what was found.
        for p in &answering {
            let Some(referent) = p.referent else {
                let found_name =
                    if p.unresolved { UNRESOLVED_FAILED } else { UNRESOLVED_DEPENDENT };
                return Spelling::Fails { found: None, found_name: found_name.to_owned() };
            };
            let origin = written::origin(self.unit, referent);
            let found = self.key(origin);
            if found != Some(ob.key) {
                let found_name = self.name(found.as_ref(), origin);
                return Spelling::Fails { found, found_name };
            }
        }
        Spelling::Holds
    }

    /// The key of `origin` under the rules, or `None` when it has none or
    /// it collides.
    fn key(&mut self, origin: DeclId) -> Option<Digest> {
        if let Some(&k) = self.origins.get(&origin) {
            return k;
        }
        let key = match self.keys.key_id(origin) {
            Ok(k) if !self.walk().collides(&k) => Some(k),
            Ok(_) | Err(_) => None,
        };
        self.origins.insert(origin, key);
        key
    }

    /// The walk's collisions and names, made on first use.
    fn walk(&mut self) -> &Collisions {
        let (input, rules) = (self.input, self.rules);
        self.walk.get_or_insert_with(|| collisions(input, rules))
    }

    /// The name of the entity found (`spec/verify.md` §6.1): the walk's
    /// name of its key's unit, else the host's name of the entity
    /// (`display_name`), which is §6.1's qualified form without what
    /// needs the walk.
    fn name(&mut self, found: Option<&Digest>, origin: DeclId) -> String {
        if let Some(k) = found
            && let Some(n) = self.walk().names.get(k)
        {
            return n.clone();
        }
        let e = self.keys.entity(origin);
        let mut n = crate::display_name(self.input.ast, e);
        if n.len() > NAME_LIMIT {
            let mut cut = NAME_LIMIT;
            while !n.is_char_boundary(cut) {
                cut -= 1;
            }
            n.truncate(cut);
        }
        n
    }
}

/// A name is cut to this many bytes (`spec/verify.md` §6.1).
const NAME_LIMIT: usize = 4096;

/// The walk of `input` under `rules`, for which keys collide and the
/// units' names.
///
/// A unit in a collision is the walk's own `collided` (WP-045), set
/// whatever cause the unit keeps first (`units::merge`), by
/// `units::mark_collisions`, `keys::mark_mapped_collisions` and
/// `units::mark_part_collisions`. Parts (a resolved or `default_argument`
/// unit, `owner` set) are left out: a part's key is of kind `Resolved` or
/// `DefaultArgument` and an origin's never is (`Keys::key_id` keys a
/// declaration by its own kind), so no obligation can have one; the unit
/// test `s3_4_part_keys_are_no_origins_keys` shows it.
///
/// A collision of a key no unit has marks the order unit's cause only
/// (it has no `collided`), and the order unit may already keep another
/// cause; so on a copy of the units the order unit's cause is put aside
/// and the mapped collisions are marked again (the units' other marks on
/// the copy are not read).
fn collisions(input: &Input<'_>, rules: &RuleTable) -> Collisions {
    let options = Options { rules, detail: Detail::None, names: true };
    crate::walk::run_with(input, &options, |w, written, output| {
        let mut keys = HashSet::new();
        let mut with_unit = HashSet::new();
        for u in written {
            if u.kind == KeyKind::Order || u.owner.is_some() {
                continue;
            }
            with_unit.insert(u.key);
            if u.collided {
                keys.insert(u.key);
            }
        }
        let mut again: Vec<crate::units::Written> = written.to_vec();
        for u in again.iter_mut().filter(|u| u.kind == KeyKind::Order) {
            u.cause = None;
        }
        crate::keys::mark_mapped_collisions(w, input, &mut again);
        let unitless =
            again.iter().any(|u| u.kind == KeyKind::Order && u.cause == Some(Cause::Collision));
        let names = output.names.into_iter().collect();
        Collisions { keys, with_unit, unitless, names }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // §10.2 rule 1: every role takes part.
    #[test]
    fn s10_2_rule1_every_role() {
        let all = [
            Role::Expr,
            Role::Member,
            Role::Nns,
            Role::Type,
            Role::Template,
            Role::Base,
            Role::MemInit,
            Role::UsingDecl,
            Role::UsingDirective,
            Role::NamespaceAlias,
            Role::Elaborated,
            Role::SpecTarget,
            Role::Concept,
            Role::Designator,
        ];
        assert!(all.iter().all(|&r| takes_part(r)));
    }

    // Digest §3.4, §12: a part's key (a resolved or `default_argument`
    // unit) is the key of no declaration, so `collisions` may leave parts
    // out. Over a unit with default arguments, a default member
    // initialiser and resolved units, under no rule and under a rename.
    #[test]
    fn s3_4_part_keys_are_no_origins_keys() {
        let src = "namespace n { struct S { int m = 1; int f(int a = 2) { return a; } };\n\
                   int f(int); template <class T> int t(T x = T()) { return f(x); } }\n\
                   int u() { n::S s; return s.f() + n::t<int>() + s.m; }\n";
        let files = [("main.cpp", src)];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        slip_kiln::snippet::parse_with(&snippet, &slip_kiln::snippet::Extra::default(), |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |_: FileId| b"main.cpp".to_vec();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let none = RuleTable::empty();
            let mut keys = Keys::new(&input);
            let f = p
                .ast
                .decls
                .ids()
                .find(|&d| crate::display_name(p.ast, d) == "n::S::f(int)")
                .expect("S::f");
            let rename = keys.rename_rule(f, b"g").expect("a rule");
            let renamed = RuleTable::new(vec![rename]).expect("a table");
            for rules in [&none, &renamed] {
                let mut keys = Keys::with_rules(&input, rules);
                let origins: HashSet<Digest> =
                    p.ast.decls.ids().filter_map(|d| keys.key_id(d).ok()).collect();
                let options = Options { rules, detail: Detail::None, names: false };
                let parts = crate::walk::run_with(&input, &options, |_, written, _| {
                    written.iter().filter(|u| u.owner.is_some()).map(|u| u.key).collect::<Vec<_>>()
                });
                assert!(parts.len() >= 3, "parts: {}", parts.len());
                assert!(parts.iter().all(|k| !origins.contains(k)));
            }
        })
        .expect("parses");
    }
}
