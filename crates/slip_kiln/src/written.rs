//! The names of a unit as the programmer wrote them (work package WP-067):
//! Kiln's written-reference table (Kiln `spec/ast.md` §18) and its
//! declaration-extent table (§19), read once, in the form `spec/edits.md`
//! §9.3 and §9.7 and `spec/verify.md` §10.2 read them. Three consumers, one
//! reading: the native rename, the spelled-entity check of the trusted
//! base and the subject of a programmable transform.
//!
//! Both tables are on only when the unit was parsed with
//! [`ParseOptions::written`](crate::ParseOptions::written) ([`recorded`]);
//! otherwise every function here returns nothing.
//!
//! **What is given.** Spans are Kiln's token spans (`[start of the first
//! token, end of the last)`, macro provenance intact); a consumer maps one
//! to bytes of a file with `slip_kiln::ranges` (WP-084), and this module
//! does not. Nothing that is not in either table is invented here: a name
//! the front end records nowhere (a designator, an overloaded operator
//! used as an operator, a using-declaration's name as a reference) is
//! absent, and is the rename's `unreachable` (`spec/edits.md` §9.5); the
//! exact list is `spec/landed/edits/WP-067.md`.
//!
//! **Order.** "Unit order" is `slip_kiln::ranges::unit_order` (WP-084)
//! once that package is on `main`; until then it is the table's own order
//! (the written-reference table's recording order, the declaration-extent
//! table's declaration-id order). The order is the one argument [`ORDER`]
//! of the functions that build the lists, so WP-084 changes one line.
//!
//! **Repeated entries.** Entries equal in role, range and referent are
//! one reference, the first kept (`docs/design/notes/kiln-landed-2026-11.md`
//! §5.3 expected Kiln to repeat the names of a default member initialiser
//! or a default argument; at this pin it records them once, so the
//! joining does not fire).
//!
//! **Dependent entries with a referent.** An entry flagged `REF_DEPENDENT`
//! that has an entity is a reference. At this pin there is none: Kiln
//! clears the flag when it patches an entity in
//! (`kiln_ast/src/written_refs.rs:270-277`). The rule is kept for when
//! Kiln records one.
//!
//! **Roles left out.** Kiln WP-2820 F1 (`20794f7ed`, in the pin since
//! WP-163) records an overloaded operator used as an operator
//! (`RefRole::Operator`) and a user-defined literal's suffix
//! (`RefRole::UdSuffix`). Kiln recorded neither at `4a83f6e3c`, and their
//! reading is a later package's (`docs/wp/WP-096.md` "For Kiln" 4,
//! `spec/api.md` §6.7). [`left_out`] is the one place that drops them:
//! every list here leaves them out, so every caller sees what it saw at
//! the old pin.
//!
//! **Destructors** are declaration names (the identifier after `~`); the
//! **origin** of a template's pattern is the template ([`origin`]).

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::Unit;
use crate::kiln::ast::decl::{DeclKind, FriendKind, FriendTarget};
use crate::kiln::ast::friend_form::{FriendFormFlags, FriendFormKind};
use crate::kiln::ast::ids::TemplateNameId;
use crate::kiln::ast::names::{DeclNameKind as NameKind, DeclNameView};
use crate::kiln::ast::templates::SpecKind;
use crate::kiln::ast::types::{Nns, TemplateName, TypeView};
use crate::kiln::ast::written_refs::{QualLink, REF_UNRESOLVED, WrittenRef as Entry, WrittenRefs};
use crate::kiln::ast::{Ast, DeclId, QualType};
use crate::kiln::lex::{LexMode, LexOptions, Lexer, Punct, TokKind};
use crate::kiln::source::symbol::Symbol;
use crate::kiln::source::{Loc, Span};

/// The syntactic role of a written name: Kiln's `RefRole`
/// (`kiln_ast/src/written_refs.rs:43`, Kiln `spec/ast.md` §18.4).
pub use crate::kiln::ast::written_refs::RefRole as Role;

/// Whether entries of role `role` are left out of everything this module
/// gives (module documentation, "Roles left out"): `Operator` and
/// `UdSuffix`, which Kiln records since WP-2820 F1 and Slip does not read
/// yet. A caller that reads Kiln's table itself asks this first.
pub fn left_out(role: Role) -> bool {
    matches!(role, Role::Operator | Role::UdSuffix)
}

/// A written reference that has a referent (Kiln's entry with an `entity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrittenRef {
    /// The syntactic role.
    pub role: Role,
    /// The span of the name alone: for a qualified name its last
    /// component; for an operator-function-id all its tokens; for `~T`
    /// the `T` (Kiln `spec/ast.md` §18.2).
    pub name: Span,
    /// The first token of the whole written name, qualifiers included:
    /// the start of the outermost qualifier component the table links
    /// to. A leading `::` is not a component: `global` says it is there,
    /// and `first` is the component after it. A component the table does
    /// not link (the one after `decltype(x)::`) ends the chain.
    pub first: Loc,
    /// What the name denotes after resolution: the table's `entity`, the
    /// function overload resolution selected (a specialisation, for a
    /// template), the underlying declaration through a using-shadow.
    pub referent: DeclId,
    /// The written name starts with `::`.
    pub global: bool,
    /// Lookup found a using-shadow (the table's `found` is a `UsingShadow`
    /// or a `ConstructorUsingShadow`). The shadow's target is not checked
    /// against the referent (Kiln records `found` only when it differs
    /// from the referent). A shadow comes from a using-declaration
    /// (`using a::f;`) **or** a using-enum-declaration: an enumerator
    /// named after `using enum E;` is `via_using`.
    pub via_using: bool,
}

/// A written name without a referent: a dependent name of a template
/// pattern (`REF_DEPENDENT`, Kiln `kiln_ast/src/written_refs.rs:138`) or
/// a name whose lookup or overload resolution failed (`REF_UNRESOLVED`,
/// `:140`).
///
/// The declarations found for it at the template's definition (the
/// overload set of an unqualified dependent call, `spec/edits.md` §9.4
/// row 1) are **not** available at this pin: Kiln's entry has no entity
/// and links no overload set, whether one overload is visible or several,
/// and in the current instantiation too (`this->a(t)`, `C::a(t)`). WP-073
/// needs that set from Kiln: a finding for Kiln (the found set of each
/// dependent entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependentName {
    /// The syntactic role.
    pub role: Role,
    /// The span of the name alone, as for [`WrittenRef::name`].
    pub name: Span,
    /// The innermost declaration whose extent holds the name (Kiln
    /// `spec/ast.md` §18.3).
    pub enclosing: Option<DeclId>,
    /// Resolution failed (`REF_UNRESOLVED`) rather than waiting for an
    /// instantiation.
    pub unresolved: bool,
}

/// How lookup found a name: Kiln's `RefLookup` (Kiln `spec/ast.md`
/// §18.7.2).
pub use crate::kiln::ast::written_refs::RefLookup as Lookup;

/// What one instantiation made a dependent name denote: a row of Kiln's
/// per-specialisation table (stage 4, Kiln WP-1933, `spec/ast.md` §18.9;
/// in the pin since WP-066), as `docs/wp/WP-139.md` item 9 reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Instance {
    /// The specialisation whose instantiation resolved the name (Kiln
    /// §18.9.2): a function template's specialisation, a member of a class
    /// template's specialisation, a variable template's specialisation; for
    /// an alias template, a default template argument or a constraint, the
    /// template or the constrained declaration.
    pub spec: DeclId,
    /// What the name denotes there (§18.2's `entity`: the underlying
    /// declaration through a using-shadow, a specialisation for a
    /// template). `None`: the name stayed unresolved in that instantiation
    /// (Kiln §18.9.3).
    pub entity: Option<DeclId>,
    /// What lookup found, when it differs from `entity` (the template of a
    /// specialisation, a using-shadow).
    pub found: Option<DeclId>,
    /// How the instantiation's lookup found it.
    pub how: Lookup,
}

/// For the span of every dependent name ([`dependent_names`]), the rows of
/// Kiln's per-specialisation table of every main-table entry with that
/// span (whatever its role), each once, sorted by `(spec, entity, found,
/// how)` so that the order of instantiation does not show (Kiln §18.9.1,
/// "Order independence"). A span absent from the map has no row: no
/// instantiation this unit made resolved it, or the table has no row for
/// its shape. Empty when the unit was parsed without `written`.
pub fn instances(unit: &Unit<'_>) -> std::collections::HashMap<Span, Vec<Instance>> {
    let ast = unit.ast();
    instances_of(&ast.written_refs, |spec, at| in_template_head(ast, spec, at))
}

/// Whether `at` lies in the template parameter list of the function
/// template whose specialisation `spec` is: a default template argument
/// (or a parameter's type) that a deduction substituted. Kiln records rows for
/// such a name since Kiln WP-3520 (`4401d1644`, in the pin since WP-163);
/// before, the deduction's frame dropped them. [`instances`] leaves them
/// out, the one place, so that such a name stays `unresolved` as it was
/// (parity): what an edit of one changes is not verified (the digest sees
/// a default template argument by its value), and reading these rows is
/// WP-154's.
pub fn in_template_head(ast: &Ast, spec: DeclId, at: Span) -> bool {
    let Some((_, info)) = ast.spec_info(spec) else { return false };
    if ast.decls.get(info.template).kind() != DeclKind::FunctionTemplate {
        return false;
    }
    let Some(t) = ast.template_info(info.template) else { return false };
    let list = ast.templates.param_list(t.params);
    list.l_angle <= at.lo && at.hi <= list.r_angle
}

fn instances_of(
    table: &WrittenRefs,
    head: impl Fn(DeclId, Span) -> bool,
) -> std::collections::HashMap<Span, Vec<Instance>> {
    let mut out: std::collections::HashMap<Span, Vec<Instance>> = std::collections::HashMap::new();
    for (_, r) in table.specs() {
        let e = table.get(r.entry);
        if e.entity.is_some() || left_out(e.role) || head(r.spec, e.range) {
            // A row's entry is a dependent one (Kiln §18.9.1); an entry
            // that has an entity is a written reference, not read here.
            continue;
        }
        out.entry(e.range).or_default().push(Instance {
            spec: r.spec,
            entity: r.entity,
            found: r.found,
            how: r.how,
        });
    }
    let key = |i: &Instance| {
        (i.spec.index(), i.entity.map(DeclId::index), i.found.map(DeclId::index), i.how as u8)
    };
    for v in out.values_mut() {
        v.sort_by_key(key);
        v.dedup();
    }
    out
}

/// What kind of declaration a name declares, as far as a rename must tell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeclNameKind {
    /// An ordinary declaration or definition (a partial specialisation
    /// included).
    Declaration,
    /// A using-declaration (`using a::f;`, a dependent one included): the
    /// name is the member it names.
    UsingDeclaration,
    /// An explicit specialisation (`template <> void f<int>(int)`), of a
    /// template or of a member of a class template specialisation.
    ExplicitSpecialisation,
    /// An explicit instantiation, a definition or a declaration
    /// (`template void f<int>(int);`, `extern template …`).
    ExplicitInstantiation,
    /// A using-directive (`using namespace n;`): its row's name is the
    /// namespace it nominates, not a name it declares. Only from [`row`].
    UsingDirective,
    /// A using-enum-declaration (`using enum E;`): its row's name is the
    /// enumeration it names, not a name it declares. Only from [`row`].
    UsingEnum,
}

/// The name of one declaration as written at it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclName {
    /// The declaration (the template, for a template: Kiln shares one row
    /// between a template and its pattern, and the pattern is left out).
    pub decl: DeclId,
    /// The identifier token of its declarator-id or declared name: for a
    /// qualified name the last component (`void N::C::f() {}`: `f`); for a
    /// specialisation written as a template-id, the template's name
    /// without its arguments; for a destructor (`~C();`, `S::~S() {}`) the
    /// identifier after `~` (as [`WrittenRef::name`] is for `~T`): a
    /// rename of the class edits it, as it edits a constructor's name; for
    /// an inheriting constructor's using-declaration the last component
    /// as written (`using AT::AT;`: the second `AT`).
    pub name: Span,
    /// What kind of declaration it is.
    pub kind: DeclNameKind,
}

/// How two names of one unit are ordered. Stable sorting with it keeps
/// the table's order among names it calls equal.
pub(crate) type Order = fn(&Unit<'_>, Span, Span) -> Ordering;

/// The order of every list here: the table's own, until
/// `slip_kiln::ranges::unit_order` (WP-084) is on `main` and takes its
/// place.
pub(crate) const ORDER: Order = table_order;

/// The table's own order: every name equal, so a stable sort changes
/// nothing.
fn table_order(_: &Unit<'_>, _: Span, _: Span) -> Ordering {
    Ordering::Equal
}

/// Whether both tables were recorded for the unit
/// ([`ParseOptions::written`](crate::ParseOptions::written)).
pub fn recorded(unit: &Unit<'_>) -> bool {
    let ast = unit.ast();
    ast.written_refs.is_on() && ast.decl_extents.is_on()
}

/// The written references of a unit that have a referent, deduplicated,
/// in unit order (module documentation, "Order"). Empty when the unit was
/// parsed without `written`.
pub fn written_refs(unit: &Unit<'_>) -> Vec<WrittenRef> {
    written_refs_in(unit, ORDER)
}

fn written_refs_in(unit: &Unit<'_>, order: Order) -> Vec<WrittenRef> {
    let ast = unit.ast();
    let shadow = |d: DeclId| {
        matches!(ast.decls.get(d).kind(), DeclKind::UsingShadow | DeclKind::ConstructorUsingShadow)
    };
    let mut out = refs_of(&ast.written_refs, shadow);
    out.sort_by(|a, b| order(unit, a.name, b.name));
    out
}

/// The entries of `table` that have a referent, deduplicated, in the
/// table's order; `shadow` says whether a declaration is a using-shadow.
fn refs_of(table: &WrittenRefs, shadow: impl Fn(DeclId) -> bool) -> Vec<WrittenRef> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (id, e) in table.iter() {
        let Some(referent) = e.entity else { continue };
        if left_out(e.role) {
            continue;
        }
        if !seen.insert((e.role, span_key(e.range), referent)) {
            continue;
        }
        let (first, global) = first_of(table, id.index(), e);
        let via_using = e.found.is_some_and(&shadow);
        out.push(WrittenRef { role: e.role, name: e.range, first, referent, global, via_using });
    }
    out
}

/// The entries without a referent, deduplicated (equal in role, range and
/// `unresolved`; the first entry's `enclosing` kept), in unit order.
pub fn dependent_names(unit: &Unit<'_>) -> Vec<DependentName> {
    dependent_names_in(unit, ORDER)
}

fn dependent_names_in(unit: &Unit<'_>, order: Order) -> Vec<DependentName> {
    let mut out = dependents_of(&unit.ast().written_refs);
    out.sort_by(|a, b| order(unit, a.name, b.name));
    out
}

/// The entries of `table` without a referent, deduplicated, in the
/// table's order.
fn dependents_of(table: &WrittenRefs) -> Vec<DependentName> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (_, e) in table.iter() {
        if e.entity.is_some() || left_out(e.role) {
            continue;
        }
        let unresolved = e.flags & REF_UNRESOLVED != 0;
        if !seen.insert((e.role, span_key(e.range), unresolved)) {
            continue;
        }
        out.push(DependentName { role: e.role, name: e.range, enclosing: e.enclosing, unresolved });
    }
    out
}

/// The name of every declaration that has an extent and a name written
/// as one identifier token (a destructor's is the identifier after `~`),
/// in unit order. A declaration without one (unnamed, an operator, a
/// conversion function, a literal operator) is absent; so are the rows that name another declaration
/// rather than declare a name (a using-directive's namespace, the enum of
/// `using enum`), a friend declaration's own row (the function or class
/// it declares has the same one), and the pattern of a template (it
/// shares the template's row).
pub fn decl_names(unit: &Unit<'_>) -> Vec<DeclName> {
    decl_names_in(unit, ORDER)
}

/// Whether declaration `d` has a row in the declaration-extent table, and
/// of which kind: read from the table for every row, those
/// [`decl_names`] leaves out included (a template's pattern, a friend
/// declaration's own row, an unnamed declaration, an operator, a
/// conversion function, a using-directive, a `using enum`). `None` when
/// `d` has no row (or the unit was parsed without `written`).
pub fn row(unit: &Unit<'_>, d: DeclId) -> Option<DeclNameKind> {
    let ast = unit.ast();
    ast.decl_extents.get(d)?;
    Some(decl_name_kind(ast, d, ast.decls.get(d).kind()))
}

fn decl_names_in(unit: &Unit<'_>, order: Order) -> Vec<DeclName> {
    let ast = unit.ast();
    let extents = &ast.decl_extents;
    let mut out = Vec::new();
    for (d, e) in extents.iter() {
        if !e.name.lo.is_valid() {
            continue;
        }
        let node = ast.decls.get(d);
        let kind = node.kind();
        if matches!(
            kind,
            DeclKind::UsingDirective
                | DeclKind::UsingEnum
                | DeclKind::Friend
                | DeclKind::FriendTemplate
                | DeclKind::LinkageSpec
                | DeclKind::Export
        ) {
            continue;
        }
        // The pattern of a template shares the template's rows (all of
        // them: since Kiln WP-2470 `iter` gives every row of a declaration
        // written more than once under one id, and `get` only the last).
        if let Some(t) = ast.described_template(d)
            && extents.shares(d, t)
        {
            continue;
        }
        let Some(ident) = identifier(unit, d) else { continue };
        let destructor = node.name().kind() == NameKind::Destructor;
        let Some(name) = identifier_span(unit, e.name, ident, destructor) else { continue };
        out.push(DeclName { decl: d, name, kind: decl_name_kind(ast, d, kind) });
    }
    out.sort_by(|a, b| order(unit, a.name, b.name));
    out
}

/// The identifier a declaration's name is written as: its own; a
/// constructor's or a destructor's class name; for an inheriting
/// constructor's using-declaration, the name its qualifier's last
/// component is written with; a deduction guide's template name. `None`
/// for an unnamed declaration and for a name that is not one identifier.
fn identifier<'r>(unit: &Unit<'r>, d: DeclId) -> Option<&'r str> {
    let ast = unit.ast();
    let name = ast.decls.get(d).name();
    let sym = match name.kind() {
        NameKind::Identifier => name.as_ident()?,
        // An inheriting constructor's using-declaration (`using A::A;`,
        // `using AT::AT;`, `using B<T>::B;`): Kiln names it as a
        // constructor of the class that declares it; what is written is
        // the name of the qualifier's last component as written (a
        // typedef's, an alias's, a template's), not the record it
        // resolves to.
        NameKind::Constructor
            if matches!(
                ast.decls.get(d).kind(),
                DeclKind::Using
                    | DeclKind::UnresolvedUsingValue
                    | DeclKind::UnresolvedUsingTypename
            ) =>
        {
            let nns = ast.using_qualifier(d)?;
            let Nns::Type { ty, .. } = ast.types.nns_view(nns) else { return None };
            written_type_name(ast, ty)?
        }
        NameKind::Constructor | NameKind::Destructor => match ast.names.view(name) {
            DeclNameView::Constructor(class) | DeclNameView::Destructor(class) => {
                let class = ast.types.as_record(QualType::unqual(class))?;
                ast.decls.get(class).name().as_ident()?
            }
            _ => return None,
        },
        NameKind::DeductionGuide => match ast.names.view(name) {
            DeclNameView::DeductionGuide(template) => ast.decls.get(template).name().as_ident()?,
            _ => return None,
        },
        NameKind::Operator | NameKind::Conversion | NameKind::LiteralOperator => return None,
    };
    let text = ast.symbols().as_str(sym);
    (!text.is_empty()).then_some(text)
}

/// The identifier type `ty` is written with, read from its sugar: a
/// typedef's or alias's name, the template's name of a template-id, a
/// class's or enumeration's name, a dependent name's identifier. `None`
/// for a type that is not written as one name.
fn written_type_name(ast: &Ast, ty: QualType) -> Option<Symbol> {
    let named = |d: DeclId| ast.decls.get(d).name().as_ident();
    match ast.types.view(ty.ty()) {
        TypeView::Typedef { decl, .. } => named(decl),
        TypeView::Using { shadow, .. } => named(shadow),
        TypeView::Record(d) | TypeView::Enum(d) | TypeView::TemplateTypeParm(d) => named(d),
        TypeView::InjectedClassName { pattern, .. } => named(pattern),
        TypeView::Elaborated { named: inner, .. } | TypeView::Paren(inner) => {
            written_type_name(ast, inner)
        }
        TypeView::TemplateSpecialization { name, .. } => written_template_name(ast, name),
        TypeView::DependentName { name, .. }
        | TypeView::DependentTemplateSpecialization { name, .. } => Some(name),
        _ => None,
    }
}

/// The identifier template name `n` is written with.
fn written_template_name(ast: &Ast, n: TemplateNameId) -> Option<Symbol> {
    match ast.types.template_name_view(n) {
        TemplateName::Template(d) | TemplateName::TemplateTemplateParm { decl: d, .. } => {
            ast.decls.get(d).name().as_ident()
        }
        TemplateName::Qualified { underlying, .. } => written_template_name(ast, underlying),
        TemplateName::DependentIdentifier { name, .. } => Some(name),
        _ => None,
    }
}

/// The span of the identifier `ident` in `name` (a declaration's
/// `DeclExtent::name`), the token as written: the token that starts at
/// `name.lo` when Kiln's lexer lexes it as the identifier `ident` (its
/// symbol: universal-character-names resolved, line splices removed) and
/// it ends at or before `name.hi` (`name` itself; or a specialisation's
/// template-id, `S<int>`, whose first token it is); for a destructor
/// (`~T`, `~ T`), the token after the `~` that starts `name`, when it is
/// the identifier `ident` and ends at `name.hi`. `None` otherwise: nothing
/// is guessed.
///
/// The identifier is compared as Kiln's symbol, not as the span's text,
/// so that a name written `caf\u00e9` or split by a line splice is
/// given with the span of its token; `slip_kiln::sites` refuses it
/// `not_plain` (its bytes are not its spelling). Before the WP-068
/// review such a declaration was left out without a word
/// (`spec/landed/edits/WP-067.md`, "Corrections").
fn identifier_span(unit: &Unit<'_>, name: Span, ident: &str, destructor: bool) -> Option<Span> {
    let symbols = unit.ast().symbols();
    let is_ident = |t: TokKind| matches!(t, TokKind::Identifier(s) if symbols.as_str(s) == ident);
    let (kind, lo, hi) = token_at(unit, name.lo)?;
    if lo != name.lo {
        return None;
    }
    if destructor {
        if kind != TokKind::Punct(Punct::Tilde) {
            return None;
        }
        let (kind, lo, end) = token_at(unit, hi)?;
        return (is_ident(kind) && end == name.hi).then_some(Span::new(lo, end));
    }
    (is_ident(kind) && hi.to_raw() <= name.hi.to_raw()).then_some(Span::new(lo, hi))
}

/// The first token at or after `at` (blanks, splices and comments
/// skipped) within `at`'s buffer or expansion entry, as Kiln's lexer lexes
/// where `at` is spelled: its kind and its two ends, as locations of
/// `at`'s buffer or entry. `None` for an invalid location, one spelled in
/// no file, at the end of the buffer, or when the token does not lie
/// within the entry.
fn token_at(unit: &Unit<'_>, at: Loc) -> Option<(TokKind, Loc, Loc)> {
    let sm = unit.sm();
    let spelled = if at.is_macro() { sm.spelling_loc(at) } else { at };
    if !spelled.is_valid() || spelled.is_macro() {
        return None;
    }
    let (file, pos) = sm.offset_of(spelled)?;
    let len = u32::try_from(sm.text(file).len()).ok()?;
    if pos >= len {
        return None;
    }
    let base = Loc::from_raw(spelled.to_raw() - pos);
    let mut lx =
        Lexer::new_in_range(sm, unit.ast().symbols(), file, pos..len, base, LexOptions::default());
    lx.set_mode(LexMode::Normal);
    let t = lx.next();
    if t.kind == TokKind::Eof {
        return None;
    }
    // From the spelling back to `at`'s buffer or entry, by distance.
    let move_to = |l: Loc| {
        let d = sm.same_entry_distance(spelled, l)?;
        let out = Loc::from_raw(at.to_raw().checked_add(d)?);
        sm.same_entry_distance(at, out).map(|_| out)
    };
    Some((t.kind, move_to(t.span.lo)?, move_to(t.span.hi)?))
}

/// What kind of declaration `d` is, as far as a rename must tell.
fn decl_name_kind(ast: &Ast, d: DeclId, kind: DeclKind) -> DeclNameKind {
    match kind {
        DeclKind::UsingDirective => return DeclNameKind::UsingDirective,
        DeclKind::UsingEnum => return DeclNameKind::UsingEnum,
        _ => {}
    }
    if matches!(
        kind,
        DeclKind::Using
            | DeclKind::UsingPack
            | DeclKind::UnresolvedUsingValue
            | DeclKind::UnresolvedUsingTypename
            | DeclKind::UnresolvedUsingIfExists
    ) {
        return DeclNameKind::UsingDeclaration;
    }
    let spec = ast
        .spec_info(d)
        .map(|(_, s)| s.kind)
        .or_else(|| ast.member_spec_info(d).map(|(_, m)| m.kind));
    match spec {
        Some(SpecKind::ExplicitSpecialization) => DeclNameKind::ExplicitSpecialisation,
        Some(
            SpecKind::ExplicitInstantiationDeclaration | SpecKind::ExplicitInstantiationDefinition,
        ) => DeclNameKind::ExplicitInstantiation,
        Some(SpecKind::Undeclared | SpecKind::ImplicitInstantiation) | None => {
            DeclNameKind::Declaration
        }
    }
}

/// The target of `Friend` `fd` as Kiln gave it at `4a83f6e3c`. Since Kiln
/// WP-3164 (`2af5deebd`, `kiln_sema/src/class/friend.rs`, the branch of a
/// special member friend matched per specialisation) a friend naming a
/// constructor, destructor or conversion function of a dependent class
/// (`friend A<T>::X::X();`) has the function type as its target and the
/// member it names as its `FriendForm` row's candidate; before, the member
/// was the target (`FriendTarget::Decl`). This gives the member again, so
/// that the digest's stream, key and name of the friend and every other
/// reader see what they saw at the old pin (WP-163, parity). Nothing else
/// of the row is read here; reading the form is WP-126's. Every reader of
/// a friend's target asks this, not `DeclArena::friend_target`.
///
/// # Panics
/// If `fd` is not a `Friend` (as `DeclArena::friend_target`).
pub fn friend_target(ast: &Ast, fd: DeclId) -> FriendTarget {
    let target = ast.decls.friend_target(fd);
    if let FriendTarget::Type { .. } = target
        && let Some(row) = ast.friend_form(fd).row()
        && matches!(
            row.kind(),
            FriendFormKind::Constructor | FriendFormKind::Destructor | FriendFormKind::Conversion
        )
        && row.has(FriendFormFlags::DEPENDENT)
        && let Some(member) = row.form().entity
    {
        return FriendTarget::Decl(member);
    }
    target
}

/// The origin of a referent (`spec/api.md` §5.5): for a specialisation of
/// a template (implicit, explicit, or explicitly instantiated) the
/// template; for a member of an instantiated class (a member function, a
/// field, an enumerator, a member template, …) the member of the
/// template it was instantiated from; for the pattern of a template (a
/// class, function, variable or alias template: the injected-class-name
/// `W` inside `template <class T> struct W`, a function template's call
/// to itself) the template, so that `W` and `W<T>` there have one origin.
/// For a partial specialisation of a class or variable template, the
/// template: its name token spells the template (`v<T*>`), and its key's
/// parent is the template's (`spec/digest.md` §3.4).
/// Repeated until none applies, so that a specialisation of a member
/// template of an instantiated class has the member template of the
/// class template as its origin. Anything else is its own origin. One
/// function, so that the rename, the host and the check cannot disagree
/// about it.
///
/// A friend declaration of an instantiated class is not taken to the
/// friend declaration of the pattern when that one does not define the
/// function: `spec/digest.md` §3.4 does not follow such a friend
/// (`spec/landed/digest/WP-103.md`, finding 2), and the function it
/// names is its own entity, so the origin of a call does not depend on
/// whether the class template was instantiated before it.
pub fn origin(unit: &Unit<'_>, referent: DeclId) -> DeclId {
    let ast = unit.ast();
    let mut d = referent;
    for _ in 0..ORIGIN_LIMIT {
        let next = match (ast.spec_info(d), ast.partial_spec_info(d)) {
            (Some((_, s)), _) => Some(s.template),
            (None, Some((_, p))) => Some(p.template),
            (None, None) => ast.described_template(d).or_else(|| {
                ast.instantiated_from(d).filter(|&p| {
                    ast.decls.get(p).misc().friend() == FriendKind::None
                        || ast.definition(p).is_some()
                })
            }),
        };
        match next {
            Some(n) if n != d => d = n,
            _ => break,
        }
    }
    d
}

/// Steps [`origin`] takes at most (each one leaves an instantiation for
/// what it was made from, so a chain is as long as templates are nested).
const ORIGIN_LIMIT: u32 = 256;

/// The start of the whole written name of entry `e` (index `at`) and
/// whether it starts with `::`: the qualifier chain walked to its end.
/// Each qualifier precedes the entry it qualifies (Kiln `spec/ast.md`
/// §18.2), so the walk ends.
fn first_of(table: &WrittenRefs, at: usize, e: &Entry) -> (Loc, bool) {
    let (mut first, mut link, mut at) = (e.range.lo, e.qualifier.link(), at);
    loop {
        match link {
            // A `decltype(...)::` or splice specifier has no entry (Kiln
            // WP-2828 links it as `Specifier`; at `4a83f6e3c` the link was
            // `None`): it ends the chain, as `WrittenRef::first` says.
            QualLink::None | QualLink::Specifier => return (first, false),
            QualLink::Global => return (first, true),
            QualLink::Entry(q) if q.index() < at => {
                let qe = table.get(q);
                first = qe.range.lo;
                link = qe.qualifier.link();
                at = q.index();
            }
            QualLink::Entry(_) => {
                debug_assert!(false, "a qualifier entry after the entry it qualifies");
                return (first, false);
            }
        }
    }
}

/// A span as a key of a set.
fn span_key(s: Span) -> (u32, u32) {
    (s.lo.to_raw(), s.hi.to_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiln::ast::written_refs::{REF_DEPENDENT, RefLookup, RefQualifier};

    fn entry(lo: u32, entity: Option<u32>, role: Role, qualifier: RefQualifier) -> Entry {
        let mut e = Entry {
            range: Span::new(Loc::from_raw(lo), Loc::from_raw(lo + 1)),
            entity: entity.map(DeclId::new),
            found: None,
            enclosing: Some(DeclId::new(1)),
            qualifier,
            role,
            flags: if entity.is_some() { 0 } else { REF_DEPENDENT },
            // Kiln WP-2130 (lookup provenance): what the other fields imply.
            how: RefLookup::None,
        };
        e.how = e.default_how();
        e
    }

    fn table(entries: &[Entry]) -> WrittenRefs {
        let mut t = WrittenRefs::default();
        t.set_on(true);
        for e in entries {
            t.push(*e);
        }
        t
    }

    // WP-067 scope 2: entries equal in role, range and referent are one,
    // WP-163: the roles Kiln records since WP-2820 F1 are left out of
    // every list, with a referent or without one, so a caller sees what it
    // saw at `4a83f6e3c`; every other role is kept.
    #[test]
    fn wp163_operator_and_ud_suffix_are_left_out() {
        let n = RefQualifier::NONE;
        let t = table(&[
            entry(10, Some(5), Role::Operator, n),
            entry(12, Some(6), Role::UdSuffix, n),
            entry(14, None, Role::Operator, n),
            entry(16, None, Role::UdSuffix, n),
            entry(18, Some(7), Role::Expr, n),
            entry(20, None, Role::Expr, n),
        ]);
        let refs: Vec<Role> = refs_of(&t, |_| false).iter().map(|r| r.role).collect();
        assert_eq!(refs, [Role::Expr]);
        let deps: Vec<Role> = dependents_of(&t).iter().map(|d| d.role).collect();
        assert_eq!(deps, [Role::Expr]);
        assert!(left_out(Role::Operator) && left_out(Role::UdSuffix));
        assert!(!left_out(Role::Expr) && !left_out(Role::Redecl) && !left_out(Role::Concept));
    }

    // the first kept; a different referent, role or range is another.
    #[test]
    fn wp067_equal_entries_are_one_reference() {
        let n = RefQualifier::NONE;
        let t = table(&[
            entry(10, Some(5), Role::Expr, n),
            entry(10, Some(5), Role::Expr, n),
            entry(10, Some(6), Role::Expr, n),
            entry(10, Some(5), Role::Member, n),
            entry(11, Some(5), Role::Expr, n),
            entry(10, Some(5), Role::Expr, n),
        ]);
        let got: Vec<(u32, u32, Role)> = refs_of(&t, |_| false)
            .iter()
            .map(|r| (r.name.lo.to_raw(), r.referent.index(), r.role))
            .collect();
        assert_eq!(
            got,
            [(10, 5, Role::Expr), (10, 6, Role::Expr), (10, 5, Role::Member), (11, 5, Role::Expr)]
        );
    }

    // WP-067 scope 3: entries without a referent, joined the same way;
    // the dependent and the unresolved are two.
    #[test]
    fn wp067_equal_dependent_entries_are_one() {
        let n = RefQualifier::NONE;
        let mut unresolved = entry(10, None, Role::Expr, n);
        unresolved.flags = REF_UNRESOLVED;
        let t = table(&[
            entry(10, None, Role::Expr, n),
            entry(10, None, Role::Expr, n),
            unresolved,
            entry(10, None, Role::Member, n),
            entry(10, Some(3), Role::Expr, n),
        ]);
        let got: Vec<(Role, bool)> =
            dependents_of(&t).iter().map(|d| (d.role, d.unresolved)).collect();
        assert_eq!(got, [(Role::Expr, false), (Role::Expr, true), (Role::Member, false)]);
        assert_eq!(refs_of(&t, |_| false).len(), 1);
    }

    // `first` and `global` walk the qualifier chain: `::a::b::f` and
    // `a::f`; `via_using` is a found using-shadow.
    #[test]
    fn wp067_first_walks_the_chain() {
        let mut t = WrittenRefs::default();
        t.set_on(true);
        let mut push = |e: Entry| RefQualifier::entry(t.push(e).expect("on"));
        let a = push(entry(10, Some(1), Role::Nns, RefQualifier::GLOBAL));
        let b = push(entry(20, Some(2), Role::Nns, a));
        let mut f = entry(30, Some(9), Role::Expr, b);
        f.found = Some(DeclId::new(8));
        push(f);
        let a2 = push(entry(40, Some(1), Role::Nns, RefQualifier::NONE));
        push(entry(50, Some(9), Role::Expr, a2));
        let got: Vec<(u32, u32, bool, bool)> = refs_of(&t, |d| d.index() == 8)
            .iter()
            .map(|r| (r.name.lo.to_raw(), r.first.to_raw(), r.global, r.via_using))
            .collect();
        assert_eq!(
            got,
            [
                (10, 10, true, false),
                (20, 10, true, false),
                (30, 10, true, true),
                (40, 40, false, false),
                (50, 40, false, false)
            ]
        );
    }
}
