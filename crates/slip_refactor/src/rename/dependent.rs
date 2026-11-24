//! Dependent names (§9.4).
//!
//! **Row 1** (the unqualified callee of a dependent call, and the
//! declarations found at the definition) waits for Kiln: K-a, the join
//! from a dependent entry to its unresolved-lookup expression
//! (`docs/requests/KR1-amendment-1.md` §2), is not in the pin
//! (`spec/landed/edits/WP-073.md`, "Kiln at the pin"). A name row 1 would
//! decide is read by row 2 when an instantiation resolved it.
//!
//! **Row 2** (`T::f`, `x.f`, `x->f`, a call found by argument-dependent
//! lookup at instantiation, `this->f` with a dependent base), per
//! instantiated specialisation, from the front end's own table: Kiln's
//! stage 4 (its WP-1933, `kiln/spec/ast.md` §18.9; in the pin since
//! `docs/wp/WP-066.md`), read through [`written::instances`]. Each row
//! says, for one dependent entry of the pattern and one specialisation
//! the observer made, what the name denotes there. `docs/wp/WP-139.md`
//! item 9: WP-127's join of the instantiation's expressions by token
//! location is gone, and is not kept as a second source.
//!
//! For one dependent name, the rows that are **evidence** are those with
//! an entity: a row without one (the name stayed unresolved in that
//! instantiation, Kiln §18.9.3) or no row at all (not instantiated, or a
//! shape the table does not record) decides nothing. Each entity is taken
//! through its using-shadow, to its origin and to the digest's entity, as
//! a written reference's is (§9.3), and looked up among the subjects.
//! Then (item 1), over every row of the observer:
//!
//! | The rows | Claim |
//! |---|---|
//! | every one binds the name to a subject | `replace` (literal piece), the holder's `changed` entry; `refuse(instantiated)` under `dependent: refuse` (item 4) |
//! | none binds a subject, and one binds a declaration named like the old name | `keep` |
//! | some bind a subject and some do not | `refuse(mixed_instances)` |
//! | no row is evidence, or no row binds a declaration named like the old name | `unresolved` |
//!
//! A row whose entity is not named like the old name is not a subject
//! and is counted with those that "do not" bind one, but alone it makes
//! no `keep`: such a row is the table's shape for something this rename
//! does not read (a type a nested-name-specifier names), not evidence
//! that the name means something else.
//!
//! Each row that is evidence at a claim the rows decided is also an
//! [`Instance`] of the section (§4.5): the claim's place, the
//! specialisation's key and display, the subjects it binds the name to.
//! Reduce joins them over observers and keeps those at an edit or at a
//! refusal of a site: the report's section of names edited on instances
//! (§10.5, item 3), and which specialisation a refusal was made on
//! (item 5).
//!
//! A dependent using-declarator (`using T::f;`, role `UsingDecl`) is not
//! claimed here: what it names is §9.2's (the mixed follower of its
//! instantiations), and its name stays `unresolved`, as before.
//!
//! A dependent entry has no referent and no found set (K4,
//! `spec/landed/edits/WP-067.md`), so nothing tells its identifier but
//! the token itself: `Sites::name_token` locates it only when its bytes
//! are the old name's spelling, and a name it does not locate is not
//! claimed (`spec/landed/edits/WP-071.md`, "Cost").
//!
//! **Cost.** Nothing when the unit has no dependent name; else one pass
//! over the per-specialisation table ([`written::instances`]), and for
//! each row of a name spelled like the old name, the entity's name and
//! its subject (a hash lookup).

use std::collections::{BTreeSet, HashMap};

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::source::Span;
use slip_kiln::written::{self, DependentName, Instance as Row, Role};
use slip_model::edits::{Instance, KeyId, Reason};
use slip_model::transform::Dependent;

use super::{Ctx, Written, decls};

/// Claims every dependent name of the unit.
pub(crate) fn claim_all(ctx: &mut Ctx<'_, '_, '_>) {
    let names = written::dependent_names(ctx.unit);
    if names.is_empty() {
        return;
    }
    let rows = written::instances(ctx.unit);
    for name in &names {
        claim(ctx, name, rows.get(&name.name).map_or(&[][..], Vec::as_slice));
    }
}

/// What one row that is evidence says.
struct Bound {
    row: Row,
    /// The subject the entity is, if it is one.
    subject: Option<KeyId>,
    /// The entity is named like the old name.
    named: bool,
}

/// Claims `name` from its rows.
fn claim(ctx: &mut Ctx<'_, '_, '_>, name: &DependentName, rows: &[Row]) {
    if name.role == Role::UsingDecl {
        ctx.claims.unresolved(&ctx.sites, name.name);
        return;
    }
    let bound: Vec<Bound> = rows.iter().filter_map(|r| bound(ctx, *r)).collect();
    let subjects: BTreeSet<KeyId> = bound.iter().filter_map(|b| b.subject).collect();
    let other = bound.iter().any(|b| b.subject.is_none());
    let other_named = bound.iter().any(|b| b.subject.is_none() && b.named);
    let subjects: Vec<KeyId> = subjects.into_iter().collect();
    match (subjects.is_empty(), other) {
        // No row binds a subject.
        (true, _) if other_named => {
            ctx.claims.keep(&ctx.sites, name.name);
            record(ctx, name.name, &bound);
        }
        (true, _) => ctx.claims.unresolved(&ctx.sites, name.name),
        // Every row binds a subject.
        (false, false) => {
            if ctx.request.dependent == Dependent::Refuse {
                ctx.claims.refuse(&ctx.sites, name.name, &subjects, Reason::Instantiated);
            } else {
                ctx.claims.replace(&ctx.sites, name.name, &subjects, Written::Literal);
                if let Some(key) = holder(ctx, name.enclosing) {
                    let mut reason = b"dependent name ".to_vec();
                    reason.extend_from_slice(ctx.old);
                    ctx.claims.changed(name.name, key, reason);
                }
            }
            record(ctx, name.name, &bound);
        }
        // Some do and some do not.
        (false, true) => {
            ctx.claims.refuse(&ctx.sites, name.name, &subjects, Reason::MixedInstances);
            record(ctx, name.name, &bound);
        }
    }
}

/// The row as evidence: `None` when it has no entity.
fn bound(ctx: &mut Ctx<'_, '_, '_>, row: Row) -> Option<Bound> {
    let ast = ctx.unit.ast();
    let e = ast.strip_shadow(row.entity?);
    let named = decls::name_of(ast, e) == Some(ctx.old);
    let subject = if named {
        let o = super::origin_of(ctx, e);
        ctx.subjects.of_decl(ctx.keys, o)
    } else {
        None
    };
    Some(Bound { row, subject, named })
}

/// The key of the unit whose stream holds a name enclosed by `enclosing`
/// (§9.4, "The holder"), if it has one.
fn holder(ctx: &mut Ctx<'_, '_, '_>, enclosing: Option<DeclId>) -> Option<KeyId> {
    let unit = ctx.keys.unit_of(enclosing?)?;
    ctx.keys.key_id(unit).ok()
}

/// Records the instances of the claim at `name`: one per row that is
/// evidence.
fn record(ctx: &mut Ctx<'_, '_, '_>, name: Span, bound: &[Bound]) {
    let mut seen: HashMap<DeclId, (Option<KeyId>, String)> = HashMap::new();
    for b in bound {
        let spec = b.row.spec;
        let (key, display) = seen
            .entry(spec)
            .or_insert_with(|| {
                (ctx.keys.key_id(spec).ok(), slip_digest::display_name(ctx.unit.ast(), spec))
            })
            .clone();
        // The place is the claim's, set when the claims are finished
        // (`Claims::finish`), as the edits' are.
        let instance = Instance {
            place: slip_model::edits::Place { file: 0, lo: 0, hi: 0 },
            spec: key,
            display,
            subjects: b.subject.into_iter().collect(),
        };
        ctx.claims.instance(name, instance);
    }
}
