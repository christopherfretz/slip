//! The subjects of a run (`spec/edits.md` §4.1, §9.2), and the flags the
//! core sets from what the unit shows (§9.5): `unreachable` for a name
//! neither of Kiln's tables records, `name_taken`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use slip_digest::query::Keys;
use slip_kiln::kiln::ast::decl::{DeclKind, DefinitionBits, NamespaceData, VarStorage};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::written::{self, DeclNameKind};
use slip_model::edits::{KeyId, Link, Role, Subject, subject_flags};
use slip_model::keys::Rule;

use super::{CTX_LIMIT, Ctx};

/// One subject.
#[derive(Debug)]
struct Entry {
    /// The entity (`Keys::entity`) it was first added by.
    entity: DeclId,
    rule: Option<Rule>,
    role: Role,
    of: BTreeSet<KeyId>,
    flags: u64,
}

/// The subjects of the observer, by key id, and their links.
#[derive(Debug, Default)]
pub(crate) struct Subjects {
    /// Entity to key id, for the entities that are subjects. Looked up,
    /// never iterated.
    by_entity: HashMap<DeclId, KeyId>,
    entries: BTreeMap<KeyId, Entry>,
    links: BTreeSet<(KeyId, KeyId)>,
}

impl Subjects {
    /// Adds the entity of `decl` (any redeclaration) or joins it. Its id
    /// is `Keys::key_id` and its rule `Keys::rename_rule(entity, new)`;
    /// without a rule it is `unsupported` (§9.5). `None` if it has no
    /// key: the caller claims nothing for it.
    ///
    /// Joined: the least role (a seed stays a seed), `of` united.
    pub fn add(
        &mut self,
        keys: &mut Keys<'_>,
        decl: DeclId,
        role: Role,
        new: &[u8],
        of: &[KeyId],
    ) -> Option<KeyId> {
        let e = keys.entity(decl);
        let id = if let Some(&id) = self.by_entity.get(&e) {
            id
        } else {
            let id = keys.key_id(e).ok()?;
            self.by_entity.insert(e, id);
            if let std::collections::btree_map::Entry::Vacant(slot) = self.entries.entry(id) {
                let (made, flags) = match keys.rename_rule(e, new) {
                    Ok(r) => (Some(r), 0),
                    Err(_) => (None, subject_flags::UNSUPPORTED),
                };
                slot.insert(Entry { entity: e, rule: made, role, of: BTreeSet::new(), flags });
            }
            id
        };
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.role = entry.role.min(role);
            entry.of.extend(of.iter().copied());
        }
        Some(id)
    }

    /// The subject `decl`'s entity is, if it is one.
    pub fn of_decl(&self, keys: &Keys<'_>, decl: DeclId) -> Option<KeyId> {
        self.by_entity.get(&keys.entity(decl)).copied()
    }

    /// Sets `flags` on subject `id`.
    pub fn flag(&mut self, id: KeyId, flags: u64) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.flags |= flags;
        }
    }

    /// The two subjects change together or not at all.
    #[allow(dead_code, reason = "for docs/wp/WP-072.md (overriding functions)")]
    pub fn link(&mut self, a: KeyId, b: KeyId) {
        if a != b {
            self.links.insert((a.min(b), a.max(b)));
        }
    }

    /// Each subject's id and entity, by id.
    pub fn entities(&self) -> Vec<(KeyId, DeclId)> {
        self.entries.iter().map(|(&id, e)| (id, e.entity)).collect()
    }

    /// Each seed's id and entity, by id.
    pub fn seeds(&self) -> Vec<(KeyId, DeclId)> {
        self.entries
            .iter()
            .filter(|(_, e)| e.role == Role::Seed)
            .map(|(&id, e)| (id, e.entity))
            .collect()
    }

    /// The section's `subjects` and `links`. `of` is kept for a follower
    /// only (§4.7).
    pub fn finish(self, ast: &Ast) -> (Vec<Subject>, Vec<Link>) {
        let subjects = self
            .entries
            .into_iter()
            .map(|(id, e)| Subject {
                id,
                rule: e.rule,
                role: e.role,
                of: if e.role == Role::Follower { e.of.into_iter().collect() } else { Vec::new() },
                flags: e.flags,
                display: slip_digest::display_name(ast, e.entity),
            })
            .collect();
        let links = self.links.into_iter().map(|(a, b)| Link { a, b }).collect();
        (subjects, links)
    }
}

/// `unreachable` (§9.5) for a declaration of a subject that neither of
/// Kiln's tables names (`spec/landed/edits/WP-067.md`, "What neither
/// table gives"): an explicit instantiation of a function or variable
/// template, which has no extent row (K1); an explicit specialisation
/// that `decl_names` does not give (one whose name a macro spells). A
/// designator never names a function or a variable, and an attribute
/// argument that is an expression is a written reference, so neither
/// sets the flag here.
///
/// A specialisation is the record `ast.templates.specs_of` lists; what is
/// written is a redeclaration of it (`ast.redecls_of`). An explicit
/// instantiation is hidden when no declaration of its specialisation has
/// an extent row of that kind; an explicit specialisation when one of its
/// declarations has a row and no `decl_names` entry, or when none of them
/// has an entry.
pub(crate) fn flag_unreachable(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let named: HashSet<DeclId> = ctx.named.old.iter().map(|d| d.decl).collect();
    for (id, e) in ctx.subjects.entities() {
        for &s in ast.templates.specs_of(e) {
            let hidden = match ast.spec_info(s).map(|(_, i)| i.kind) {
                Some(
                    SpecKind::ExplicitInstantiationDeclaration
                    | SpecKind::ExplicitInstantiationDefinition,
                ) => !ast
                    .redecls_of(s)
                    .any(|x| written::row(unit, x) == Some(DeclNameKind::ExplicitInstantiation)),
                Some(SpecKind::ExplicitSpecialization) => {
                    let mut any = false;
                    let mut lost = false;
                    for x in ast.redecls_of(s) {
                        let has = named.contains(&x);
                        any |= has;
                        lost |= !has && written::row(unit, x).is_some();
                    }
                    lost || !any
                }
                _ => false,
            };
            if hidden {
                ctx.subjects.flag(id, subject_flags::UNREACHABLE);
            }
        }
    }
}

/// `name_taken` (§9.5): the scope a subject's name enters declares an
/// entity named like the new name, or a macro of that name is defined in
/// the unit. Decided from what this observer shows:
///
/// - the scopes a name enters ([`entered`]): its semantic scope, and
///   outwards through every transparent one (an unscoped enumeration, an
///   anonymous union or struct, a linkage specification, `export`, an
///   inline namespace); a block-scope `extern` declaration enters its
///   namespace. A template parameter enters none: it is no member of the
///   scope its template is in;
/// - for a class, its bases at any depth: the subject's name is taken
///   when one of its scopes, or a base of one, is a scope the new name
///   enters; and when one of its scopes is a base of a class that declares
///   the new name (a class derived from the subject's, which this observer
///   sees, hides the renamed member);
/// - a macro named like the new name, defined anywhere in the unit
///   (`Unit::pp_record`, when the unit was parsed with it): every
///   subject.
///
/// A local or a template parameter named like the new name in a function
/// that uses the subject is not `name_taken`: the spelled-entity check of
/// the verifier is the guard there (§9.5).
pub(crate) fn flag_name_taken(ctx: &mut Ctx<'_, '_, '_>) {
    let ast = ctx.unit.ast();
    if macro_named(ctx.unit, ctx.new) {
        for (id, _) in ctx.subjects.entities() {
            ctx.subjects.flag(id, subject_flags::NAME_TAKEN);
        }
        return;
    }
    if ctx.named.new.is_empty() {
        return;
    }
    // The scopes the new name enters, and those with their bases.
    let mut taken: HashSet<DeclId> = HashSet::new();
    let mut taken_bases: HashSet<DeclId> = HashSet::new();
    for d in &ctx.named.new {
        for s in entered(ast, ctx.keys, d.decl) {
            taken.insert(s);
            taken_bases.extend(with_bases(ast, ctx.keys, s));
        }
    }
    for (id, e) in ctx.subjects.entities() {
        let scopes = entered(ast, ctx.keys, e);
        let hit = scopes.iter().any(|&s| {
            taken_bases.contains(&s)
                || with_bases(ast, ctx.keys, s).iter().any(|x| taken.contains(x))
        });
        if hit {
            ctx.subjects.flag(id, subject_flags::NAME_TAKEN);
        }
    }
}

/// Whether the preprocessing record of `unit` has a definition of a macro
/// named `name` (any definition, in effect at the end or not).
fn macro_named(unit: &slip_kiln::Unit<'_>, name: &[u8]) -> bool {
    let Some(record) = unit.pp_record() else { return false };
    let symbols = unit.ast().symbols();
    record.defs().iter().any(|d| symbols.as_str(d.def.name()).as_bytes() == name)
}

/// The scopes the name of declaration `d` enters, each as its entity
/// (`Keys::entity`), innermost first.
fn entered(ast: &Ast, keys: &Keys<'_>, d: DeclId) -> Vec<DeclId> {
    let node = ast.decls.get(d);
    if matches!(
        node.kind(),
        DeclKind::TemplateTypeParm | DeclKind::NonTypeTemplateParm | DeclKind::TemplateTemplateParm
    ) {
        return Vec::new();
    }
    let Some(mut s) = node.ctx() else { return Vec::new() };
    if is_local_extern(ast, d) {
        // Its entity is a member of the innermost enclosing namespace.
        for _ in 0..CTX_LIMIT {
            if matches!(ast.decls.get(s).kind(), DeclKind::Namespace | DeclKind::TranslationUnit) {
                break;
            }
            let Some(up) = ast.decls.get(s).ctx() else { break };
            s = up;
        }
    }
    let mut out = Vec::new();
    for _ in 0..CTX_LIMIT {
        out.push(keys.entity(s));
        let transparent = match ast.decls.get(s).kind() {
            DeclKind::LinkageSpec | DeclKind::Export => true,
            DeclKind::Enum => ast.decls.enumeration(s).is_some_and(|e| !e.is_scoped()),
            DeclKind::Namespace => ast.decls.namespace(s).is_some_and(NamespaceData::is_inline),
            _ => ast.decls.record(s).is_some_and(|r| r.defn.contains(DefinitionBits::ANONYMOUS)),
        };
        if !transparent {
            break;
        }
        let Some(up) = ast.decls.get(s).ctx() else { break };
        s = up;
    }
    out
}

/// A function, or a variable declared `extern`, declared at block scope
/// (as the digest tells one, `Walk::is_local_extern`).
fn is_local_extern(ast: &Ast, d: DeclId) -> bool {
    let n = ast.decls.get(d);
    if n.misc().local_extern() {
        return true;
    }
    let candidate = n.kind() == DeclKind::Function
        || (n.kind() == DeclKind::Var
            && ast.decls.var(d).is_some_and(|v| v.spec.storage() == VarStorage::Extern));
    candidate
        && n.ctx().is_some_and(|c| {
            matches!(ast.decls.get(c).kind(), DeclKind::Function | DeclKind::CXXMethod)
                || ast.decls.function(c).is_some()
        })
}

/// `scope`, and when it is a class every base class of it at any depth,
/// each as its entity.
fn with_bases(ast: &Ast, keys: &Keys<'_>, scope: DeclId) -> Vec<DeclId> {
    let mut out = vec![scope];
    let mut seen: HashSet<DeclId> = HashSet::from([scope]);
    let mut at = 0;
    while at < out.len() && at < CTX_LIMIT as usize {
        let r = out[at];
        at += 1;
        if ast.decls.record(r).is_none() {
            continue;
        }
        // The bases are those of the class's definition.
        let def = ast.definition(r).unwrap_or(r);
        for b in ast.decls.bases(def, &ast.lists) {
            if let Some(base) = ast.types.as_record(ast.types.canonical(b.ty)) {
                let base = keys.entity(base);
                if seen.insert(base) {
                    out.push(base);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use slip_digest::query::Keys;
    use slip_kiln::ParseOptions;
    use slip_kiln::tree::Tree;
    use slip_kiln::written;
    use slip_model::edits::{Role, subject_flags};

    use super::Subjects;

    /// §9.5: an entity `Keys::rename_rule` gives no rule for (here an
    /// explicit specialisation, which no rule matches, `spec/verify.md`
    /// §2.1) is a subject without a rule and `unsupported`. A run never
    /// adds such an entity at this pin (`origin` takes a specialisation to
    /// its template), so the contract of `add` is asked directly.
    #[test]
    fn an_entity_without_a_rule_is_unsupported() {
        let src =
            b"namespace n { template <class T> void f(T) {} template <> void f<int>(int) {} }\n";
        // Under the target directory, beside the test binary (a unit test
        // has no `CARGO_TARGET_TMPDIR`).
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        let tree = Tree::new(dir, "subjects-no-rule", &[("main.cpp", &src[..])]).unwrap();
        let obs = tree.observer("main.cpp", "c++20", &[]);
        let mut o = ParseOptions::default();
        o.written = true;
        let (_, r) = tree
            .parse(&obs, &o, |u| {
                let ast = u.ast();
                let spec = written::decl_names(u)
                    .into_iter()
                    .map(|d| d.decl)
                    .find(|&d| ast.spec_info(d).is_some())
                    .expect("the explicit specialisation");
                let subject = |p: &[u8]| p.to_vec();
                slip_digest::map::with_input(u, &subject, |input| {
                    let mut keys = Keys::new(input);
                    let mut s = Subjects::default();
                    let id = s.add(&mut keys, spec, Role::Seed, b"g", &[]).expect("a key");
                    let e = &s.entries[&id];
                    (e.rule.is_none(), e.flags)
                })
            })
            .unwrap();
        let (no_rule, flags) = r.unwrap().unwrap();
        assert!(no_rule);
        assert_eq!(flags, subject_flags::UNSUPPORTED);
    }
}
