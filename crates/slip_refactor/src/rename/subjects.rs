//! The subjects of a run (`spec/edits.md` §4.1, §9.2), and the flags the
//! core sets from what the unit shows (§9.5): `unreachable` for a name
//! neither of Kiln's tables records, `name_taken`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use slip_digest::query::Keys;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::written::{self, DeclNameKind};
use slip_model::edits::{KeyId, Link, Role, Subject, subject_flags};
use slip_model::keys::Rule;

use super::{CTX_LIMIT, Ctx, enclosing};

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
        let id = match self.by_entity.get(&e) {
            Some(&id) => id,
            None => {
                let id = keys.key_id(e).ok()?;
                self.by_entity.insert(e, id);
                if !self.entries.contains_key(&id) {
                    let (rule, flags) = match keys.rename_rule(e, new) {
                        Ok(r) => (Some(r), 0),
                        Err(_) => (None, subject_flags::UNSUPPORTED),
                    };
                    self.entries.insert(
                        id,
                        Entry { entity: e, rule, role, of: BTreeSet::new(), flags },
                    );
                }
                id
            }
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
        self.entries.iter().filter(|(_, e)| e.role == Role::Seed).map(|(&id, e)| (id, e.entity)).collect()
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
pub(crate) fn flag_unreachable(ctx: &mut Ctx<'_, '_, '_>) {
    let unit = ctx.unit;
    let ast = unit.ast();
    let named: HashSet<DeclId> = ctx
        .named
        .old
        .iter()
        .filter(|d| d.kind == DeclNameKind::ExplicitSpecialisation)
        .map(|d| d.decl)
        .collect();
    for (id, e) in ctx.subjects.entities() {
        for &s in ast.templates.specs_of(e) {
            let hidden = match ast.spec_info(s).map(|(_, i)| i.kind) {
                Some(
                    SpecKind::ExplicitInstantiationDeclaration
                    | SpecKind::ExplicitInstantiationDefinition,
                ) => written::row(unit, s).is_none(),
                Some(SpecKind::ExplicitSpecialization) => !named.contains(&s),
                _ => false,
            };
            if hidden {
                ctx.subjects.flag(id, subject_flags::UNREACHABLE);
            }
        }
    }
}

/// `name_taken` (§9.5): the scope that declares a subject declares an
/// entity named like the new name: for a namespace, any part of it this
/// observer sees (all parts have one entity); for a class, the class and
/// its bases. Decided from the declarations `decl_names` gives.
pub(crate) fn flag_name_taken(ctx: &mut Ctx<'_, '_, '_>) {
    if ctx.named.new.is_empty() {
        return;
    }
    let ast = ctx.unit.ast();
    let mut taken: HashSet<DeclId> = HashSet::new();
    for d in &ctx.named.new {
        if let Some(s) = enclosing(ast, d.decl) {
            taken.insert(ctx.keys.entity(s));
        }
    }
    for (id, e) in ctx.subjects.entities() {
        let Some(s) = enclosing(ast, e) else { continue };
        let scopes = with_bases(ast, ctx.keys, ctx.keys.entity(s));
        if scopes.iter().any(|x| taken.contains(x)) {
            ctx.subjects.flag(id, subject_flags::NAME_TAKEN);
        }
    }
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
