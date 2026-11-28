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
//! Then (WP-139 item 1, WP-152 items 1 and 2), over every row of the
//! observer:
//!
//! | The rows | Claim |
//! |---|---|
//! | every one binds the name to a subject, and no instance captures `new_name` | `replace` (literal piece); `refuse(instantiated)` under `dependent: refuse` |
//! | every one binds the name to a subject, and some instance's lookup for `new_name` finds a non-subject | `refuse(captured_at_instance)` with a `note` report naming the specialisation and the declaration found |
//! | none binds a subject, and one binds a declaration named like the old name | `keep` |
//! | some bind a subject and some do not | `refuse(mixed_instances)` |
//! | no row is evidence, or no row binds a declaration named like the old name | `unresolved` |
//!
//! **Per-instance capture check** (`docs/wp/WP-152.md`). For every
//! instance of a row-2 `replace`, the lookup the instantiation made for
//! the old name is repeated for `new_name` in the same context, as far as
//! Kiln exposes that context at this pin:
//!
//! - **Argument-dependent lookup** (`Lookup::Adl`, `Lookup::Both`):
//!   `SpecRef::adl` gives the exact associated set (`AdlSet`: namespaces,
//!   classes, enums). `Ast::lookup_local(ns, new_name)` is read in every
//!   associated namespace for functions and function templates; an
//!   `ADL_ONLY` hidden friend is included only when a declaration of it
//!   is lexically in an associated class (`[basic.lookup.argdep]/4`).
//!   Any non-subject found is a capture.
//! - **Member access** (`Lookup::Member`: `t.f`, `p->f`, `this->f`,
//!   `t.B::f`) and **qualified name** (`Lookup::Qualified`: `T::f`,
//!   `&T::f`, `D::f`): Kiln's per-specialisation table records the entity
//!   found, not the lookup's context class. Where `spec`'s instantiated
//!   AST keeps the rebuilt `MemberExpr` / `DeclRefExpr` at the dependent
//!   name's location (`body`, constructor mem-initializers, field/var
//!   initializers, class `static_assert`s), its qualifier or object class
//!   is read from that expression and searched by post-image
//!   `[class.member.lookup]` ([`member_lookup_post`]). Shapes whose
//!   substituted expression is not kept in `spec` (alias templates,
//!   requires-clauses, substituted signatures) stay accepted and not
//!   verified (listed under "For Kiln" in `spec/landed/edits/WP-152.md`).
//! - **Hiding vs capture (Item 2)**: [`member_lookup_post`] simulates
//!   `[class.member.lookup]` in the **post-image** (where every subject
//!   declared or introduced in a class `c` now has `new_name`). A class
//!   `c` that already declares a non-subject `new_name` stops and reports
//!   a capture; a class `c` that declares no non-subject `new_name` and
//!   declares/introduces a subject stops without recursing into `c`'s
//!   bases, so a base's `new_name` hidden by the renamed entity is not a
//!   capture. Virtual-base dominance (`[class.member.lookup]/6`) drops a
//!   non-subject found solely through a virtual base dominated by a
//!   subject's class; a non-dominated sibling or derived class is a
//!   capture.
//!
//! Row 2 is read only under `dependent: offer` or `refuse`. Under
//! `leave`, the request's default until dependent names are compared
//! mapped (`docs/wp/WP-154.md`), every dependent name is `unresolved`
//! (residue `dependent`), as before this package. A `replace` here gives
//! no `changed` entry: no unit is withdrawn from the comparison for a
//! token (WP-139's second review), so the holder of an edited name
//! differs and the run is not verified, loudly, until then.
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
//! its subject (a hash lookup). The capture check runs only when every
//! row binds a subject and `new_name` is an interned symbol of the AST.

use std::collections::{BTreeSet, HashMap, HashSet};

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::{CastKind, ExprFlags, ExprKind};
use slip_kiln::kiln::ast::lookup::LookupFlags;
use slip_kiln::kiln::ast::names::DeclName;
use slip_kiln::kiln::ast::stmt::StmtKind;
use slip_kiln::kiln::ast::types::{Nns, TypeView};
use slip_kiln::kiln::ast::{Ast, DeclId, ExprId, NnsId, QualType, StmtId};
use slip_kiln::kiln::source::{Loc, Span};
use slip_kiln::written::{self, AdlSetId, DependentName, Instance as Row, Lookup, Role};
use slip_model::edits::{Instance, KeyId, Reason, Report, Severity};
use slip_model::transform::Dependent;

use super::{CTX_LIMIT, Ctx, Written, decls};

/// Maximum AST nodes visited when searching one specialisation for the
/// rebuilt expression of a dependent member or qualified name.
const WALK_LIMIT: usize = 100_000;

/// Maximum inheritance depth searched during post-image member lookup.
const BASE_DEPTH_LIMIT: u32 = 64;

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

/// One capture found in an instantiation: the specialisation and the
/// non-subject declaration `new_name` finds there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Capture {
    spec: DeclId,
    found: DeclId,
}

/// Claims `name` from its rows.
fn claim(ctx: &mut Ctx<'_, '_, '_>, name: &DependentName, rows: &[Row]) {
    // `dependent: leave` (the default): row 2 is not read (§9.4).
    if name.role == Role::UsingDecl || ctx.request.dependent == Dependent::Leave {
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
                let found = captures(ctx, name, &bound);
                if found.is_empty() {
                    // No `changed` entry: no unit is withdrawn from the
                    // comparison for a token (WP-139's second review). The
                    // holder differs, and the run is not verified, until
                    // dependent names are compared mapped (§9.4).
                    ctx.claims.replace(&ctx.sites, name.name, &subjects, Written::Literal);
                } else {
                    ctx.claims.refuse(&ctx.sites, name.name, &subjects, Reason::CapturedAtInstance);
                    record_captures(ctx, name.name, &found);
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

/// Records one `Severity::Note` report per unique capture at `name`,
/// naming the specialisation and the non-subject declaration found
/// (`docs/wp/WP-152.md` item 1).
fn record_captures(ctx: &mut Ctx<'_, '_, '_>, name: Span, found: &[Capture]) {
    let ast = ctx.unit.ast();
    let new_text = String::from_utf8_lossy(ctx.new);
    let mut seen = HashSet::new();
    for c in found {
        let key = (ast.first(c.spec), ast.first(c.found));
        if !seen.insert(key) {
            continue;
        }
        let spec_display = slip_digest::display_name(ast, c.spec);
        let found_display = slip_digest::display_name(ast, c.found);
        let message = format!("in {spec_display}: new name '{new_text}' finds {found_display}");
        ctx.claims.report(
            name,
            Report {
                place: None,
                severity: Severity::Note,
                message: message.into_bytes(),
                attached: false,
            },
        );
    }
}

/// Checks every instance of a row-2 `replace` for a non-subject
/// declaration named `new_name` that the instance's lookup would find
/// (`docs/wp/WP-152.md` items 1 and 2).
fn captures(ctx: &mut Ctx<'_, '_, '_>, name: &DependentName, bound: &[Bound]) -> Vec<Capture> {
    let ast = ctx.unit.ast();
    let Ok(new_str) = std::str::from_utf8(ctx.new) else { return Vec::new() };
    let Some(new_sym) = ast.symbols().get(new_str) else {
        // `new_name` is not even an interned symbol in this unit: no
        // declaration anywhere in the AST can be named `new_name`.
        return Vec::new();
    };
    let new_dn = DeclName::ident(new_sym);
    let old_dn =
        std::str::from_utf8(ctx.old).ok().and_then(|s| ast.symbols().get(s)).map(DeclName::ident);

    let mut out = Vec::new();
    for b in bound {
        check_row(ctx, name.name.lo, b.row, old_dn, new_dn, &mut out);
    }
    out
}

/// Checks one `SpecRef` row for a capture of `new_dn`.
fn check_row(
    ctx: &mut Ctx<'_, '_, '_>,
    name_loc: Loc,
    row: Row,
    old_dn: Option<DeclName>,
    new_dn: DeclName,
    out: &mut Vec<Capture>,
) {
    let Some(entity) = row.entity else { return };
    match row.how {
        Lookup::Member => check_member(ctx, name_loc, row, entity, old_dn, new_dn, out),
        Lookup::Qualified => check_qualified(ctx, name_loc, row, entity, old_dn, new_dn, out),
        Lookup::Adl | Lookup::Both => {
            if let Some(adl_id) = row.adl {
                check_adl(ctx, row.spec, adl_id, new_dn, out);
            }
        }
        Lookup::Unqualified | Lookup::Builtin | Lookup::None | Lookup::Injected => {}
    }
}

/// Whether `d` (a non-shadow declaration) is a subject of the rename.
fn is_subject(ctx: &mut Ctx<'_, '_, '_>, d: DeclId) -> bool {
    let o = super::origin_of(ctx, d);
    ctx.subjects.of_decl(ctx.keys, o).is_some()
}

/// Member access (`t.f`, `p->f`, `this->f`, `t.B::f`): reads the lookup
/// class (qualifier class if qualified, else object class) from the
/// rebuilt `MemberExpr` in `row.spec` and runs post-image class member
/// lookup for `new_dn`.
fn check_member(
    ctx: &mut Ctx<'_, '_, '_>,
    name_loc: Loc,
    row: Row,
    entity: DeclId,
    old_dn: Option<DeclName>,
    new_dn: DeclName,
    out: &mut Vec<Capture>,
) {
    let classes =
        lookup_classes_in_spec(ctx.unit.ast(), row.spec, name_loc, entity, Lookup::Member);
    for class in classes {
        check_class(ctx, row.spec, class, old_dn, new_dn, out);
    }
}

/// Qualified name (`T::f`, `&T::f`, `D::f`): reads the nominated class
/// from the rebuilt `DeclRefExpr` (or qualified `MemberExpr`) in
/// `row.spec` and runs post-image class member lookup for `new_dn`.
fn check_qualified(
    ctx: &mut Ctx<'_, '_, '_>,
    name_loc: Loc,
    row: Row,
    entity: DeclId,
    old_dn: Option<DeclName>,
    new_dn: DeclName,
    out: &mut Vec<Capture>,
) {
    let classes =
        lookup_classes_in_spec(ctx.unit.ast(), row.spec, name_loc, entity, Lookup::Qualified);
    for class in classes {
        check_class(ctx, row.spec, class, old_dn, new_dn, out);
    }
}

/// Argument-dependent lookup (`[basic.lookup.argdep]/4`): looks up
/// `new_dn` in every associated namespace of `adl_id` for functions and
/// function templates, admitting an `ADL_ONLY` hidden friend only when a
/// declaration of it is lexically in an associated class. Any non-subject
/// found is a capture.
fn check_adl(
    ctx: &mut Ctx<'_, '_, '_>,
    spec: DeclId,
    adl_id: AdlSetId,
    new_dn: DeclName,
    out: &mut Vec<Capture>,
) {
    let ast = ctx.unit.ast();
    let adl = ast.written_refs.adl_set_of(adl_id);
    let namespaces: Vec<DeclId> = adl.namespaces().to_vec();
    let classes: Vec<DeclId> = adl.classes().to_vec();
    let mut found = Vec::new();
    for ns in namespaces {
        for (raw, flags) in ast.lookup_local(ns, new_dn).iter_with_flags() {
            let d = ast.strip_shadow(raw);
            let k = ast.decls.get(d).kind();
            if !matches!(k, DeclKind::Function | DeclKind::FunctionTemplate) {
                continue;
            }
            if flags.contains(LookupFlags::ADL_ONLY) && !friend_of_any(ast, d, &classes) {
                continue;
            }
            found.push(d);
        }
    }
    for d in found {
        if !is_subject(ctx, d) {
            out.push(Capture { spec, found: d });
        }
    }
}

/// True when some declaration of `d` (or of `d`'s pattern if it is a
/// function template) is lexically in one of `classes` (a hidden friend,
/// `[basic.lookup.argdep]/4.2`; mirrors `kiln_sema::overload::adl`).
fn friend_of_any(ast: &Ast, d: DeclId, classes: &[DeclId]) -> bool {
    let in_assoc =
        |x: DeclId| ast.decls.lexical_ctx(x).is_some_and(|l| classes.contains(&ast.first(l)));
    if in_assoc(d) {
        return true;
    }
    let mut cur = Some(ast.template_info(d).and_then(|t| t.pattern).unwrap_or(d));
    for _ in 0..CTX_LIMIT {
        let Some(x) = cur else { break };
        if in_assoc(x) {
            return true;
        }
        cur = ast.prev(x);
    }
    false
}

/// What one subobject class in `[class.member.lookup]` supplies for
/// `new_name` in the post-image.
struct Provider {
    /// First declaration of the class supplying the name.
    class: DeclId,
    /// Every path from the starting class to this subobject crossed at
    /// least one virtual base specifier (so a derived class with `class`
    /// as a virtual base dominates it, `[class.member.lookup]/6`).
    via_virtual: bool,
    /// `None`: this class supplies only the renamed subject(s).
    /// `Some(decls)`: this class supplies non-subject declaration(s)
    /// already named `new_name`.
    non_subjects: Option<Vec<DeclId>>,
}

/// Checks `class` with post-image `[class.member.lookup]` for `new_dn`:
/// any non-subject provider that is not dominated by a subject provider
/// is a capture; a `new_name` declared only in a base hidden by a
/// subject's class (or dominated via virtual inheritance) is not.
fn check_class(
    ctx: &mut Ctx<'_, '_, '_>,
    spec: DeclId,
    class: DeclId,
    old_dn: Option<DeclName>,
    new_dn: DeclName,
    out: &mut Vec<Capture>,
) {
    let mut providers = Vec::new();
    member_lookup_post(ctx, class, old_dn, new_dn, false, 0, &mut providers);
    let ast = ctx.unit.ast();
    let subject_classes: Vec<DeclId> =
        providers.iter().filter(|p| p.non_subjects.is_none()).map(|p| p.class).collect();
    for p in &providers {
        let Some(decls) = &p.non_subjects else { continue };
        // `[class.member.lookup]/6`: a non-subject found solely through a
        // virtual base dominated by a class that supplies the renamed
        // subject is hidden by the renamed subject.
        if p.via_virtual
            && subject_classes
                .iter()
                .any(|&sc| sc != p.class && is_virtual_base_of(ast, sc, p.class))
        {
            continue;
        }
        for &d in decls {
            out.push(Capture { spec, found: d });
        }
    }
}

/// Post-image `[class.member.lookup]` in `class` for `new_dn`
/// (`docs/wp/WP-152.md` item 2):
///
/// 1. If `class` directly declares or introduces (via a non-hidden
///    using-shadow) any non-subject named `new_dn`, lookup in `class`'s
///    subobject stops here and reports those non-subjects.
/// 2. Else if `class` directly declares or introduces (via a non-hidden
///    using-shadow) a subject named `old_dn` (which will be named
///    `new_dn` in the post-image), lookup in `class`'s subobject stops
///    here and does **not** recurse into `class`'s bases: any `new_name`
///    in `class`'s bases is hidden by the renamed entity.
/// 3. Otherwise, recurse into every base class of `class`'s definition.
fn member_lookup_post(
    ctx: &mut Ctx<'_, '_, '_>,
    class: DeclId,
    old_dn: Option<DeclName>,
    new_dn: DeclName,
    via_virtual: bool,
    depth: u32,
    out: &mut Vec<Provider>,
) {
    if depth >= BASE_DEPTH_LIMIT {
        return;
    }
    let ast = ctx.unit.ast();
    let c = ast.first(class);
    let own_new = visible_members(ast, c, new_dn);
    let non_subjects: Vec<DeclId> = own_new.into_iter().filter(|&d| !is_subject(ctx, d)).collect();
    if !non_subjects.is_empty() {
        out.push(Provider { class: c, via_virtual, non_subjects: Some(non_subjects) });
        return;
    }
    if let Some(old_dn) = old_dn {
        let own_old = visible_members(ctx.unit.ast(), c, old_dn);
        if own_old.into_iter().any(|d| is_subject(ctx, d)) {
            out.push(Provider { class: c, via_virtual, non_subjects: None });
            return;
        }
    }
    let ast = ctx.unit.ast();
    let def = ast.definition(c).unwrap_or(c);
    let bases: Vec<(DeclId, bool)> = ast
        .decls
        .bases(def, &ast.lists)
        .filter_map(|b| {
            let base = ast.types.as_record(ast.types.canonical(b.ty))?;
            Some((base, b.is_virtual))
        })
        .collect();
    for (base, is_virt) in bases {
        member_lookup_post(ctx, base, old_dn, new_dn, via_virtual || is_virt, depth + 1, out);
    }
}

/// The visible members `class` directly declares or introduces named
/// `name` (skipping using-declarations themselves and hidden
/// using-shadows, taking non-hidden shadows to their targets).
fn visible_members(ast: &Ast, class: DeclId, name: DeclName) -> Vec<DeclId> {
    let mut out = Vec::new();
    for d in ast.lookup_local(class, name).iter() {
        let target = match ast.decls.get(d).kind() {
            DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow => {
                if ast.shadow_hider(d).is_some() {
                    continue;
                }
                ast.strip_shadow(d)
            }
            DeclKind::Using
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename
            | DeclKind::UnresolvedUsingIfExists => continue,
            _ => d,
        };
        if !out.contains(&target) {
            out.push(target);
        }
    }
    out
}

/// True when `base` (first declaration) is a virtual base of `derived`
/// (first declaration) at any depth.
fn is_virtual_base_of(ast: &Ast, derived: DeclId, base: DeclId) -> bool {
    let mut seen: HashSet<(DeclId, bool)> = HashSet::new();
    let mut stack = vec![(derived, false)];
    let mut steps = 0usize;
    while let Some((cur, virt)) = stack.pop() {
        if steps >= CTX_LIMIT as usize || !seen.insert((cur, virt)) {
            continue;
        }
        steps += 1;
        let def = ast.definition(cur).unwrap_or(cur);
        for b in ast.decls.bases(def, &ast.lists) {
            let Some(bc) = ast.types.as_record(ast.types.canonical(b.ty)).map(|r| ast.first(r))
            else {
                continue;
            };
            let v = virt || b.is_virtual;
            if bc == base && v {
                return true;
            }
            stack.push((bc, v));
        }
    }
    false
}

/// Finds the lookup class(es) for the dependent name at `name_loc`
/// resolving to `entity` with lookup kind `how` inside specialisation
/// `spec`'s instantiated AST.
fn lookup_classes_in_spec(
    ast: &Ast,
    spec: DeclId,
    name_loc: Loc,
    entity: DeclId,
    how: Lookup,
) -> Vec<DeclId> {
    let target = ast.first(ast.strip_shadow(entity));
    let mut stmts = Vec::new();
    let mut exprs = Vec::new();
    seed_spec_roots(ast, spec, &mut stmts, &mut exprs);
    if stmts.is_empty() && exprs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen_stmts: HashSet<StmtId> = HashSet::new();
    let mut seen_exprs: HashSet<ExprId> = HashSet::new();
    let mut steps = 0usize;

    while (!stmts.is_empty() || !exprs.is_empty()) && steps < WALK_LIMIT {
        while let Some(e) = exprs.pop() {
            if steps >= WALK_LIMIT || !seen_exprs.insert(e) {
                continue;
            }
            steps += 1;
            if let Some(cls) = match_expr_class(ast, e, name_loc, target, how)
                && !out.contains(&cls)
            {
                out.push(cls);
            }
            let kind = ast.exprs.get(e).kind();
            if kind == ExprKind::LambdaExpr {
                if let Some(body) = ast.exprs.lambda(e).body {
                    stmts.push(body);
                }
            } else if kind == ExprKind::StmtExpr {
                stmts.push(ast.exprs.stmt_expr_body(e));
            }
            ast.exprs.children_into(e, &ast.lists, &mut exprs);
        }
        if let Some(st) = stmts.pop() {
            if steps >= WALK_LIMIT || !seen_stmts.insert(st) {
                continue;
            }
            steps += 1;
            push_stmt_children(ast, st, &mut stmts, &mut exprs);
        }
    }
    out
}

/// Seeds `stmts` and `exprs` with the AST roots owned by specialisation
/// `spec` (function body, constructor mem-initializers, variable/field
/// initializers, class `static_assert`s).
fn seed_spec_roots(ast: &Ast, spec: DeclId, stmts: &mut Vec<StmtId>, exprs: &mut Vec<ExprId>) {
    let def = ast.definition(spec).unwrap_or(spec);
    if let Some(f) = ast.decls.function(def) {
        if let Some(body) = f.body {
            stmts.push(body);
        }
        for init in ast.ctor_inits(def) {
            exprs.push(init.init);
        }
        return;
    }
    if let Some(v) = ast.decls.var(def) {
        if let Some(init) = v.init {
            exprs.push(init);
        }
        return;
    }
    if let Some(f) = ast.decls.field(def) {
        if let Some(init) = f.init {
            exprs.push(init);
        }
        if let Some(bw) = f.bit_width {
            exprs.push(bw);
        }
        return;
    }
    if ast.decls.record(def).is_some() {
        for m in ast.members(def) {
            match ast.decls.get(m).kind() {
                DeclKind::StaticAssert => {
                    if let Some(sa) = ast.decls.static_assert(m) {
                        exprs.push(sa.cond);
                        if let Some(msg) = sa.message {
                            exprs.push(msg);
                        }
                    }
                }
                DeclKind::Field => {
                    if let Some(f) = ast.decls.field(m) {
                        if let Some(init) = f.init {
                            exprs.push(init);
                        }
                        if let Some(bw) = f.bit_width {
                            exprs.push(bw);
                        }
                    }
                }
                DeclKind::Var => {
                    if let Some(v) = ast.decls.var(m)
                        && let Some(init) = v.init
                    {
                        exprs.push(init);
                    }
                }
                _ => {}
            }
        }
    }
}

/// If `e` is the rebuilt `MemberExpr` or `DeclRefExpr` at `name_loc`
/// resolving to `target`, returns the class that lookup searched.
fn match_expr_class(
    ast: &Ast,
    e: ExprId,
    name_loc: Loc,
    target: DeclId,
    how: Lookup,
) -> Option<DeclId> {
    let n = ast.exprs.get(e);
    if n.loc() != name_loc {
        return None;
    }
    match (how, n.kind()) {
        (Lookup::Member, ExprKind::MemberExpr) => {
            if ast.first(ast.strip_shadow(ast.exprs.decl(e))) != target {
                return None;
            }
            if let Some(nns) = ast.exprs.decl_ref_extra(e).and_then(|x| x.qualifier) {
                return nns_record(ast, nns);
            }
            let base = unpeel_object_base(ast, ExprId::try_new(n.a())?);
            let ty = ast.exprs.get(base).ty();
            record_of_base_type(ast, ty, n.has(ExprFlags::ARROW))
        }
        (Lookup::Qualified, ExprKind::DeclRefExpr | ExprKind::MemberExpr) => {
            if ast.first(ast.strip_shadow(ast.exprs.decl(e))) != target {
                return None;
            }
            let nns = ast.exprs.decl_ref_extra(e).and_then(|x| x.qualifier)?;
            nns_record(ast, nns)
        }
        _ => None,
    }
}

/// Peels implicit derived-to-base and qualification adjustments Sema
/// inserted on a `MemberExpr`'s base expression when converting the
/// object to the member's declaring class, recovering the object's class
/// that member lookup actually searched.
fn unpeel_object_base(ast: &Ast, mut e: ExprId) -> ExprId {
    for _ in 0..32 {
        let n = ast.exprs.get(e);
        match n.kind() {
            ExprKind::ImplicitCastExpr => match ast.exprs.cast_kind(e) {
                CastKind::DerivedToBase
                | CastKind::UncheckedDerivedToBase
                | CastKind::NoOp
                | CastKind::LValueToRValue => e = ast.exprs.operand(e),
                _ => break,
            },
            ExprKind::MaterializeTemporaryExpr | ExprKind::ParenExpr => {
                let Some(inner) = ExprId::try_new(n.a()) else { break };
                e = inner;
            }
            _ => break,
        }
    }
    e
}

/// The class named by nested-name-specifier `nns`.
fn nns_record(ast: &Ast, nns: NnsId) -> Option<DeclId> {
    let Nns::Type { ty, .. } = ast.types.nns_view(nns) else { return None };
    ast.types.as_record(ast.types.canonical(ty)).map(|r| ast.first(r))
}

/// The class of a member access's base expression of type `ty` (`arrow`
/// for `->`, peeling references and the pointer).
fn record_of_base_type(ast: &Ast, ty: QualType, arrow: bool) -> Option<DeclId> {
    let mut q = ast.types.canonical(ty);
    if let TypeView::LValueRef(inner) | TypeView::RValueRef(inner) = ast.types.view(q.ty()) {
        q = ast.types.canonical(inner);
    }
    if arrow {
        q = ast.types.canonical(ast.types.pointee(q)?);
    }
    ast.types.as_record(q).map(|r| ast.first(r))
}

/// Appends the child statements and expressions of `st` to `stmts` and
/// `exprs`.
fn push_stmt_children(ast: &Ast, st: StmtId, stmts: &mut Vec<StmtId>, exprs: &mut Vec<ExprId>) {
    let n = ast.stmts.get(st);
    match n.kind() {
        StmtKind::CompoundStmt => {
            stmts.extend(ast.lists.iter::<StmtId>(ast.stmts.compound_body(st)));
        }
        StmtKind::DeclStmt => {
            for d in ast.lists.iter::<DeclId>(ast.stmts.decl_stmt_decls(st)) {
                push_local_decl(ast, d, exprs);
            }
        }
        StmtKind::ExprStmt => exprs.push(ast.stmts.expr_stmt_expr(st)),
        StmtKind::NullStmt | StmtKind::BreakStmt | StmtKind::ContinueStmt | StmtKind::GotoStmt => {}
        StmtKind::IfStmt => {
            let d = ast.stmts.if_data(st);
            stmts.extend(d.init);
            push_opt_decl(ast, d.cond_var, exprs);
            exprs.extend(d.cond);
            stmts.extend(d.then);
            stmts.extend(d.els);
        }
        StmtKind::SwitchStmt => {
            let d = ast.stmts.switch_data(st);
            stmts.extend(d.init);
            push_opt_decl(ast, d.cond_var, exprs);
            exprs.push(d.cond);
            stmts.push(d.body);
        }
        StmtKind::CaseStmt => {
            let d = ast.stmts.case_data(st);
            exprs.push(d.lhs);
            exprs.extend(d.rhs);
            stmts.push(d.sub);
        }
        StmtKind::DefaultStmt => stmts.push(ast.stmts.default_sub(st)),
        StmtKind::WhileStmt | StmtKind::DoStmt | StmtKind::ForStmt => {
            let d = ast.stmts.loop_data(st);
            stmts.extend(d.init);
            push_opt_decl(ast, d.cond_var, exprs);
            exprs.extend(d.cond);
            exprs.extend(d.inc);
            stmts.extend(d.body);
        }
        StmtKind::ReturnStmt => exprs.extend(ast.stmts.return_value(st)),
        StmtKind::LabelStmt | StmtKind::AttributedStmt => stmts.push(ast.stmts.sub_stmt(st)),
        StmtKind::IndirectGotoStmt => exprs.push(ast.stmts.indirect_goto_target(st)),
        StmtKind::CXXForRangeStmt => {
            let d = ast.stmts.range_for_data(st);
            stmts.extend(d.init);
            push_local_decl(ast, d.range, exprs);
            push_opt_decl(ast, d.begin, exprs);
            push_opt_decl(ast, d.end, exprs);
            exprs.extend(d.cond);
            exprs.extend(d.inc);
            push_local_decl(ast, d.loop_var, exprs);
            stmts.push(d.body);
        }
        StmtKind::CXXExpansionStmt => {
            let d = ast.stmts.expansion_data(st);
            stmts.extend(d.init);
            if ast.stmts.expansion_expanded(st) {
                stmts.extend(ast.lists.iter::<StmtId>(d.prelude));
                stmts.extend(ast.lists.iter::<StmtId>(d.expansions));
            } else {
                exprs.extend(ast.lists.iter::<ExprId>(d.exprs));
                exprs.extend(d.range_init);
                stmts.push(d.body);
            }
        }
        StmtKind::CXXTryStmt => {
            stmts.push(ast.stmts.try_block(st));
            stmts.extend(ast.lists.iter::<StmtId>(ast.stmts.try_handlers(st)));
        }
        StmtKind::CXXCatchStmt => {
            push_opt_decl(ast, ast.stmts.catch_var(st), exprs);
            stmts.push(ast.stmts.catch_block(st));
        }
        StmtKind::CoroutineBodyStmt => {
            let d = ast.stmts.coroutine_body(st);
            stmts.push(d.body);
            stmts.push(d.promise_stmt);
            exprs.push(d.init_suspend);
            exprs.push(d.final_suspend);
            stmts.extend(d.on_exception);
            stmts.extend(d.on_fallthrough);
        }
        StmtKind::CoreturnStmt => {
            exprs.extend(ast.stmts.coreturn_operand(st));
            exprs.extend(ast.stmts.coreturn_promise_call(st));
        }
        StmtKind::GCCAsmStmt => {
            for op in ast.stmts.asm_operands(st) {
                exprs.push(op.expr);
            }
        }
        StmtKind::ContractAssertStmt => exprs.push(ast.stmts.contract_cond(st)),
    }
}

fn push_opt_decl(ast: &Ast, d: Option<DeclId>, exprs: &mut Vec<ExprId>) {
    if let Some(d) = d {
        push_local_decl(ast, d, exprs);
    }
}

/// Appends the expressions directly owned by a local declaration inside a
/// statement (`Var`, `StaticAssert`, `Decomposition`).
fn push_local_decl(ast: &Ast, d: DeclId, exprs: &mut Vec<ExprId>) {
    if let Some(v) = ast.decls.var(d) {
        exprs.extend(v.init);
    } else if let Some(sa) = ast.decls.static_assert(d) {
        exprs.push(sa.cond);
        exprs.extend(sa.message);
    }
}
