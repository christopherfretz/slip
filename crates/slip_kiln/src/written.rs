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
//! the front end records nowhere (an attribute argument, a designator, a
//! using-declaration's name as a reference) is absent, and is the rename's
//! `unreachable` (`spec/edits.md` §9.5).
//!
//! **Order.** "Unit order" is `slip_kiln::ranges::unit_order` (WP-084)
//! once that package is on `main`; until then it is the table's own order
//! (the written-reference table's recording order, the declaration-extent
//! table's declaration-id order). The order is the one argument [`ORDER`]
//! of the functions that build the lists, so WP-084 changes one line.
//!
//! **Repeated entries.** Kiln records a name of a default member
//! initialiser or a default argument again when it parses it again
//! (`docs/design/notes/kiln-landed-2026-11.md` §5.3): entries equal in
//! role, range and referent are one reference, the first kept.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::Unit;
use crate::kiln::ast::decl::DeclKind;
use crate::kiln::ast::names::{DeclNameKind as NameKind, DeclNameView};
use crate::kiln::ast::templates::SpecKind;
use crate::kiln::ast::types::Nns;
use crate::kiln::ast::written_refs::{QualLink, REF_UNRESOLVED, WrittenRef as Entry, WrittenRefs};
use crate::kiln::ast::{Ast, DeclId, QualType};
use crate::kiln::lex::{LexOptions, token_end};
use crate::kiln::source::{Loc, Span};

/// The syntactic role of a written name: Kiln's `RefRole`
/// (`kiln_ast/src/written_refs.rs:43`, Kiln `spec/ast.md` §18.4).
pub use crate::kiln::ast::written_refs::RefRole as Role;

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
    /// Lookup found a using-shadow (the table's `found`) whose target is
    /// the referent.
    pub via_using: bool,
}

/// A written name without a referent: a dependent name of a template
/// pattern (`REF_DEPENDENT`, Kiln `kiln_ast/src/written_refs.rs:138`) or
/// a name whose lookup or overload resolution failed (`REF_UNRESOLVED`,
/// `:140`).
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
    /// without its arguments.
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
        if e.entity.is_some() {
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
/// as one identifier token, in unit order. A declaration without one
/// (unnamed, an operator, a conversion function, a destructor, a literal
/// operator) is absent; so are the rows that name another declaration
/// rather than declare a name (a using-directive's namespace, the enum of
/// `using enum`), a friend declaration's own row (the function or class
/// it declares has the same one), and the pattern of a template (it
/// shares the template's row).
pub fn decl_names(unit: &Unit<'_>) -> Vec<DeclName> {
    decl_names_in(unit, ORDER)
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
        // The pattern of a template shares the template's row.
        if let Some(t) = ast.described_template(d)
            && extents.get(t).is_some_and(|row| std::ptr::eq(row, e))
        {
            continue;
        }
        let Some(ident) = identifier(unit, d) else { continue };
        let Some(name) = identifier_span(unit, e.name, ident) else { continue };
        out.push(DeclName { decl: d, name, kind: decl_name_kind(ast, d, kind) });
    }
    out.sort_by(|a, b| order(unit, a.name, b.name));
    out
}

/// The identifier a declaration's name is written as: its own, a
/// constructor's class name (for an inheriting constructor's
/// using-declaration, the base its qualifier names), a deduction
/// guide's template name. `None`
/// for an unnamed declaration and for a name that is not one identifier.
fn identifier<'r>(unit: &Unit<'r>, d: DeclId) -> Option<&'r str> {
    let ast = unit.ast();
    let name = ast.decls.get(d).name();
    let sym = match name.kind() {
        NameKind::Identifier => name.as_ident()?,
        // An inheriting constructor's using-declaration (`using A::A;`):
        // Kiln names it as a constructor of the class that declares it;
        // what is written is the base's name, the class the qualifier's
        // last component names (`identifier_span` checks the text).
        NameKind::Constructor if ast.decls.get(d).kind() == DeclKind::Using => {
            let nns = ast.using_qualifier(d)?;
            let Nns::Type { ty, .. } = ast.types.nns_view(nns) else { return None };
            let base = ast.types.as_record(ty)?;
            ast.decls.get(base).name().as_ident()?
        }
        NameKind::Constructor => match ast.names.view(name) {
            DeclNameView::Constructor(class) => {
                let class = ast.types.as_record(QualType::unqual(class))?;
                ast.decls.get(class).name().as_ident()?
            }
            _ => return None,
        },
        NameKind::DeductionGuide => match ast.names.view(name) {
            DeclNameView::DeductionGuide(template) => ast.decls.get(template).name().as_ident()?,
            _ => return None,
        },
        NameKind::Operator
        | NameKind::Destructor
        | NameKind::Conversion
        | NameKind::LiteralOperator => return None,
    };
    let text = ast.symbols().as_str(sym);
    (!text.is_empty()).then_some(text)
}

/// The span of the identifier `ident` at the start of `name` (a
/// declaration's `DeclExtent::name`): `name` itself when it is the
/// identifier, else (a specialisation's template-id, `S<int>`) its first
/// token as Kiln's `token_end` ends it, when that token's text is `ident`
/// byte for byte. `None` otherwise: nothing is guessed.
fn identifier_span(unit: &Unit<'_>, name: Span, ident: &str) -> Option<Span> {
    let sm = unit.sm();
    if sm.span_text(name) == Some(ident) {
        return Some(name);
    }
    let end = token_end(sm, unit.ast().symbols(), name.lo, LexOptions::default())?;
    let first = Span::new(name.lo, end);
    (end.to_raw() < name.hi.to_raw() && sm.span_text(first) == Some(ident)).then_some(first)
}

/// What kind of declaration `d` is, as far as a rename must tell.
fn decl_name_kind(ast: &Ast, d: DeclId, kind: DeclKind) -> DeclNameKind {
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

/// The origin of a referent (`spec/api.md` §5.5): for a specialisation of
/// a template (implicit, explicit, or explicitly instantiated) the
/// template; for a member of an instantiated class (a member function, a
/// field, an enumerator, a member template, …) the member of the
/// template it was instantiated from; repeated until neither applies, so
/// that a specialisation of a member template of an instantiated class
/// has the member template of the class template as its origin. Anything
/// else is its own origin. One function, so that the rename, the host and
/// the check cannot disagree about it.
pub fn origin(unit: &Unit<'_>, referent: DeclId) -> DeclId {
    let ast = unit.ast();
    let mut d = referent;
    for _ in 0..ORIGIN_LIMIT {
        let next = match ast.spec_info(d) {
            Some((_, s)) => Some(s.template),
            None => ast.instantiated_from(d),
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
            QualLink::None => return (first, false),
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
    use crate::kiln::ast::written_refs::{REF_DEPENDENT, RefQualifier};

    fn entry(lo: u32, entity: Option<u32>, role: Role, qualifier: RefQualifier) -> Entry {
        Entry {
            range: Span::new(Loc::from_raw(lo), Loc::from_raw(lo + 1)),
            entity: entity.map(DeclId::new),
            found: None,
            enclosing: Some(DeclId::new(1)),
            qualifier,
            role,
            flags: if entity.is_some() { 0 } else { REF_DEPENDENT },
        }
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
