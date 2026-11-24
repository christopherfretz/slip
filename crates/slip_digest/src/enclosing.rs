//! What an entity takes from its enclosing namespaces besides its name
//! (`spec/digest-kinds.md` §1.7 row `enclosing`, §5.1 item 6; WP-113).
//!
//! A key's parent chain says which namespaces enclose an entity, and a
//! rule rewrites that chain (`spec/digest.md` §3.4). What code generation
//! and reflection take from the enclosing namespaces other than their
//! names must therefore be in the entity's unit, written as the
//! post-image has it, or a rule could move it with the name
//! (`spec/verify.md` §2.1). That is:
//!
//! - the linkage of the entity's name as Kiln computes it
//!   (`Ast::linkage`, [basic.link]/3-4): internal in or under an unnamed
//!   namespace, module linkage inside a named module unless exported;
//!   what reflection's `has_*_linkage` reads;
//! - whether an unnamed namespace encloses it (`Ast::in_anonymous_namespace`),
//!   which Kiln's mangler and code generation read with it
//!   (`kiln_abi::mangle::has_internal_linkage`);
//!   it parts from `Ast::linkage` for a function or variable first
//!   declared in an `extern "C"` context of an unnamed namespace, which
//!   keeps external linkage as in clang (Kiln WP-2623/2624,
//!   `Ast::extern_c_escapes_unnamed_namespace`; at `913a490bb` it was a
//!   block-scope `extern` declaration in a function of an unnamed
//!   namespace, now internal);
//! - the visibility that the nearest enclosing namespace opening with a
//!   `visibility` (or, for a type's visibility, a `type_visibility`)
//!   attribute gives each declaration, found as clang finds it: from the
//!   opening the declaration is written in, or, for one written qualified
//!   outside its namespace, the latest opening of the namespace written
//!   before it ([`opening`]), then through each opening's lexical context
//!   (the opening it is written in). The attribute is the opening's own,
//!   not the namespace's: one opening with it and one without give their
//!   members different visibility. Declarations whose pairs differ make
//!   the unit indeterminate (`Cause::VisibilityByUse`): a use's symbol
//!   takes the pair of the declaration most recent at the use.
//!
//! An entity inside a class or a function takes all of these from that
//! class or function (`spec/landed/digest/WP-113.md`, "The chain"), whose
//! unit holds this row, and does not get it.

use slip_kiln::kiln::ast::attr::{AttrKind, AttrNode};
use slip_kiln::kiln::ast::decl::{DeclKind, TemplLinkKind};
use slip_kiln::kiln::ast::linkage::Linkage;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{AttrId, DeclId};

use crate::Cause;
use crate::keys::Role;
use crate::stream::Stream;
use crate::tables::{linkage, walk_row};
use crate::walk::Walk;

/// A visibility word that is not 0, 1 or 2 (`AttrNode` `VISIBILITY.a`).
const BAD_VISIBILITY: Cause = Cause::Unaccounted("enclosing/visibility");
/// A declaration of a namespace's entity written before every opening of
/// the namespace (not in a valid program).
const NO_OPENING: Cause = Cause::Unaccounted("enclosing/opening");
/// A chain of contexts longer than any AST has.
const CHAIN_LIMIT: Cause = Cause::Unaccounted("enclosing/chain");
/// How far a chain of contexts is followed.
const CTX_LIMIT: u32 = 4096;

/// Whether the unit of `e` holds the row: `e` is not a namespace, and
/// its semantic context, through linkage specifications and `export`, is
/// a namespace or the translation unit, or it is a block-scope `extern`
/// declaration, which is keyed in its namespace (digest-kinds §2.4).
///
/// A namespace has no symbol; its own linkage is read by reflection
/// alone (`has_internal_linkage(^^n)`), whose result is a value of the
/// unit that asks. And its unit is one for every namespace a rule merges
/// onto it (`spec/verify.md` §2.1).
fn at_namespace_scope(w: &Walk<'_>, e: DeclId) -> bool {
    let ast = w.ast;
    if ast.decls.get(e).kind() == DeclKind::Namespace {
        return false;
    }
    // A declaration a scope may repeat (a using-declaration and its
    // shadows, a namespace alias, `using enum`, a friend declaration) and
    // a content-keyed unit declare no symbol; repeated in openings of
    // different visibility (libstdc++'s `<cwchar>` repeats `using
    // ::mbstate_t;` in plain and attributed openings of `std`) they are
    // one entity only while their digests are equal (digest §3.4).
    match crate::keys::info(ast.decls.get(e).kind()).role {
        Role::Entity(k) if crate::units::repeatable(k) => return false,
        Role::Content(_) => return false,
        Role::Entity(_)
        | Role::Nothing
        | Role::Transparent
        | Role::Root
        | Role::Local
        | Role::InParent
        | Role::Unsupported(_) => {}
    }
    let rep = w.representative(e);
    if w.is_local_extern(rep) {
        return true;
    }
    match ast.semantic_parent(rep) {
        None => true,
        Some(c) => {
            matches!(ast.decls.get(c).kind(), DeclKind::TranslationUnit | DeclKind::Namespace)
        }
    }
}

/// The row `enclosing` of the unit of `e`, at the end of its stream;
/// nothing for an entity that is not at namespace scope.
pub(crate) fn write(w: &mut Walk<'_>, s: &mut Stream, e: DeclId) {
    if !at_namespace_scope(w, e) {
        return;
    }
    let ast = w.ast;
    let rep = w.representative(e);
    s.uv(u64::from(walk_row::ENCLOSING));
    let row = match ast.linkage(rep) {
        Linkage::None => linkage::NONE,
        Linkage::Internal => linkage::INTERNAL,
        Linkage::Module => linkage::MODULE,
        Linkage::External => linkage::EXTERNAL,
    };
    s.tag(row);
    s.bool(ast.in_anonymous_namespace(rep));
    // The visibility pair. A specialisation takes it from its template,
    // except an explicit specialisation of a function template, which
    // takes it from its own declarations ([`governing_template`]).
    let pair = match governing_template(w, rep) {
        // A variable template gives its first declaration's (clang++-19:
        // `template <class T> extern int v;` in a hidden opening, defined in
        // a plain one, gives `HIDDEN _ZN1n1vIiEE`, and the reverse
        // `DEFAULT`); a class or function template the declaration most
        // recent where it is instantiated, which no unit holds when its
        // declarations differ.
        Governs::Template(t) if ast.decls.get(t).kind() == DeclKind::VarTemplate => {
            chain_pair(w, s, t, |d| d == ast.first(t))
        }
        Governs::Template(t) => chain_pair(w, s, t, |_| true),
        Governs::Own(Some(t)) => {
            // Kiln's chain of a function template's explicit
            // specialisation holds the declaration it instantiated from
            // the template, at the location of a declaration of the
            // template or of its pattern: not one written for the
            // specialisation, and clang has no such declaration.
            let made: Vec<_> = std::iter::once(t)
                .chain(ast.redecls_of(t))
                .flat_map(|d| [Some(d), ast.template_info(d).and_then(|i| i.pattern)])
                .flatten()
                .map(|d| ast.decls.get(d).loc())
                .collect();
            chain_pair(w, s, e, |d| !made.contains(&ast.decls.get(d).loc()))
        }
        // A variable template itself: what its specialisations take.
        Governs::Own(None) if ast.decls.get(e).kind() == DeclKind::VarTemplate => {
            chain_pair(w, s, e, |d| d == ast.first(e))
        }
        Governs::Own(None) => {
            let kind = ast.decls.get(e).kind();
            let definition = crate::units::defining_decl(w, e);
            match definition {
                // A class or an enumeration takes its definition's
                // opening, wherever it is used, even before the definition
                // (clang++-19: `g<n::S *>()` instantiated before `struct
                // n::S` is defined takes the definition's opening, as do
                // `_ZTI`, `_ZTS` and the vtable).
                Some(def) if is_tag(kind) => chain_pair(w, s, e, |d| d == def),
                // A function or variable this translation unit does not
                // define has only undefined references, whose visibility is
                // an opening's attribute or default: `-fvisibility` is not
                // applied to them (clang++-19, `-fvisibility=hidden`: `UND
                // DEFAULT` from a plain opening), so a plain opening and a
                // `visibility("default")` one give the same.
                None if crate::keys::info(kind).function || kind == DeclKind::Var => {
                    chain_pair_with(w, s, e, |_| true, true)
                }
                Some(_) | None => chain_pair(w, s, e, |_| true),
            }
        }
    };
    visibility(s, pair.0);
    visibility(s, pair.1);
}

/// Whose declarations give entity `rep` its visibility (clang++-19,
/// `spec/landed/digest/WP-113.md`, "Specialisations"): a specialisation
/// of a class or variable template, of any kind, a partial
/// specialisation, and an instantiation (implicit or explicit) of a
/// function template take their template's, whatever opening they are
/// written in; an explicit specialisation of a function template takes
/// its own (with the template, whose declarations Kiln's chain holds);
/// anything else its own.
enum Governs {
    Template(DeclId),
    Own(Option<DeclId>),
}

fn governing_template(w: &Walk<'_>, rep: DeclId) -> Governs {
    let ast = w.ast;
    let first = ast.first(rep);
    if let Some((_, i)) = ast.spec_info(first) {
        let function = ast.decls.function(first).is_some();
        return match i.kind {
            SpecKind::ExplicitSpecialization if function => Governs::Own(Some(i.template)),
            SpecKind::ExplicitSpecialization
            | SpecKind::ImplicitInstantiation
            | SpecKind::ExplicitInstantiationDeclaration
            | SpecKind::ExplicitInstantiationDefinition
            | SpecKind::Undeclared => Governs::Template(i.template),
        };
    }
    if let Some((TemplLinkKind::PartialSpecialization, i)) = ast.templ_link(first).get()
        && let Some(p) = ast.templates.partial(i)
    {
        return Governs::Template(p.template);
    }
    Governs::Own(None)
}

/// The visibility pair of the declarations of entity `e` that `keep`
/// holds of (all of them if it holds of none), each from [`opening`]. A
/// use's symbol takes the pair of the declaration most recent at the use
/// (clang), so pairs that differ make the symbol's visibility a fact of
/// each use's place, which no unit holds: the unit is indeterminate
/// (`VisibilityByUse`, digest §10). Otherwise the one pair they share:
/// written per declaration it would add only their number, which no
/// symbol depends on (`equal/_/var-extern-before-definition`,
/// `differ/_/key-extern-c-in-two-namespaces`).
fn chain_pair(w: &Walk<'_>, s: &mut Stream, e: DeclId, keep: impl Fn(DeclId) -> bool) -> Pair {
    chain_pair_with(w, s, e, keep, false)
}

/// Whether declarations of kind `kind` declare a class or an enumeration.
/// A class template's specialisations (`ClassTemplateSpecialization`,
/// `ClassTemplatePartialSpecialization`) never ask: their template
/// governs them ([`governing_template`]).
fn is_tag(kind: DeclKind) -> bool {
    matches!(kind, DeclKind::CXXRecord | DeclKind::Enum)
}

/// [`chain_pair`]; with `undefined`, an absent visibility is read as
/// default (0) before the pairs are compared and written.
fn chain_pair_with(
    w: &Walk<'_>,
    s: &mut Stream,
    e: DeclId,
    keep: impl Fn(DeclId) -> bool,
    undefined: bool,
) -> Pair {
    let mut chain = w.redecls(e);
    chain.reverse();
    if chain.iter().any(|&d| keep(d)) {
        chain.retain(|&d| keep(d));
    }
    // A friend declaration (not a definition) instantiated from a class
    // template counts only when the entity has no other declaration
    // (clang++-19: `int h(int);` in a plain opening, `friend int h(int);`
    // in a class template of a hidden one, instantiated, then a use gives
    // `UND DEFAULT`, and the reverse `UND HIDDEN`;
    // `rules/enclosing/friend-declared-beside-namespace-declaration`).
    let definition = crate::units::defining_decl(w, e);
    let instantiated = |d: DeclId| written(w, d) != d && Some(d) != definition;
    if chain.iter().any(|&d| !instantiated(d)) {
        chain.retain(|&d| !instantiated(d));
    }
    let pairs: Vec<Pair> = chain
        .iter()
        .map(|&d| namespace_visibility(w, s, d))
        .map(|(v, t)| if undefined { (v.or(Some(0)), t.or(Some(0))) } else { (v, t) })
        .collect();
    let pair = pairs.first().copied().unwrap_or((None, None));
    if pairs.iter().any(|&p| p != pair) {
        s.indeterminate(Cause::VisibilityByUse);
    }
    pair
}

/// A value visibility and a type visibility.
type Pair = (Option<u32>, Option<u32>);

/// `opt<uv>`: a visibility, 0 default, 1 hidden, 2 protected.
fn visibility(s: &mut Stream, v: Option<u32>) {
    match v {
        Some(v) => {
            s.opt(true);
            s.uv(u64::from(v));
            if v > 2 {
                s.indeterminate(BAD_VISIBILITY);
            }
        }
        None => {
            s.opt(false);
        }
    }
}

/// The visibility the nearest enclosing namespace opening of declaration
/// `d` with a `visibility` attribute gives it, and the type visibility
/// the nearest one with a `type_visibility` or `visibility` attribute
/// gives it (Kiln's `vis_of` on a namespace: `type_visibility` first),
/// from the opening [`opening`] gives, outward through each opening's
/// lexical context.
fn namespace_visibility(w: &Walk<'_>, s: &mut Stream, d: DeclId) -> Pair {
    if let Ok(start) = opening(w, d) {
        walk(w, s, start).unwrap_or((None, None))
    } else {
        s.indeterminate(NO_OPENING);
        (None, None)
    }
}

/// Where the walk of declaration `d` starts: clang's semantic context.
///
/// For `d` whose semantic parent is a namespace, that is an opening of
/// the namespace: the one `d` is written in (through linkage
/// specifications and `export`), else, for a declaration written
/// qualified outside it (`int n::f() {}`, `struct n::S {…};`, a friend
/// `int n::f(S) {}`, `int a::b::f() {}` in an opening of `a`), the latest
/// opening of the namespace written before `d` (clang++-19: `namespace n
/// { int f(); } namespace [[hidden]] n { } int n::f() {}` is hidden, the
/// attributes the other way round default). Openings are compared by
/// `DeclId`, which Kiln gives in the order declarations are written. No
/// such opening: `Err`. An entity at file scope has none (`Ok(None)`);
/// one in a function (a block-scope `extern`) starts at its lexical
/// context.
fn opening(w: &Walk<'_>, d: DeclId) -> Result<Option<DeclId>, ()> {
    let ast = w.ast;
    let d = written(w, d);
    let node = ast.decls.get(d);
    let Some(sem) = ast.semantic_parent(d) else { return Ok(None) };
    match ast.decls.get(sem).kind() {
        DeclKind::TranslationUnit => Ok(None),
        DeclKind::Namespace => {
            let mut l = ast.decls.lexical_ctx(d);
            for _ in 0..CTX_LIMIT {
                match l.map(|x| ast.decls.get(x).kind()) {
                    Some(DeclKind::LinkageSpec | DeclKind::Export) => {
                        l = l.and_then(|x| ast.decls.lexical_ctx(x));
                    }
                    _ => break,
                }
            }
            if let Some(x) = l
                && ast.decls.get(x).kind() == DeclKind::Namespace
                && ast.first(x) == ast.first(sem)
            {
                return Ok(Some(x));
            }
            ast.redecls_of(sem).filter(|&o| o < d).max().map(Some).ok_or(())
        }
        _ => Ok(ast.decls.lexical_ctx(d).or_else(|| node.ctx())),
    }
}

/// The declaration `d` was instantiated from, through every level
/// (`Ast::instantiated_from`; for a template, its pattern's): a
/// declaration made when a class template is instantiated (a friend
/// defined or declared in it, a friend function template) is in no
/// opening, and its `DeclId` is where the instantiation happened. clang
/// gives it the opening its pattern is written in (clang++-19, with and
/// without `-fvisibility=hidden`: `HIDDEN _ZN1n1fENS_1SIiEE` for a friend
/// defined in a class template of a hidden opening, whether or not a
/// plain opening of `n` comes before the instantiation;
/// `rules/enclosing/friend-instantiated-after-plain-reopening`). `d`
/// itself for a declaration that is written.
fn written(w: &Walk<'_>, mut d: DeclId) -> DeclId {
    let ast = w.ast;
    for _ in 0..CTX_LIMIT {
        let from = ast.instantiated_from(d).or_else(|| {
            ast.template_info(d).and_then(|i| i.pattern).and_then(|p| ast.instantiated_from(p))
        });
        match from {
            Some(p) if p != d => d = p,
            Some(_) | None => return d,
        }
    }
    d
}

/// The two visibilities from context `c` outwards; `None` when the chain
/// reaches the translation unit without passing a namespace.
fn walk(w: &Walk<'_>, s: &mut Stream, mut c: Option<DeclId>) -> Option<(Option<u32>, Option<u32>)> {
    let ast = w.ast;
    let (mut value, mut ty) = (None, None);
    let mut seen = false;
    for _ in 0..CTX_LIMIT {
        let Some(x) = c else { return seen.then_some((value, ty)) };
        let kind = ast.decls.get(x).kind();
        if kind == DeclKind::TranslationUnit {
            return seen.then_some((value, ty));
        }
        if kind == DeclKind::Namespace {
            seen = true;
            let own = attr_of(w, x, AttrKind::VISIBILITY);
            if value.is_none() {
                value = own;
            }
            if ty.is_none() {
                ty = attr_of(w, x, AttrKind::TYPE_VISIBILITY).or(own);
            }
            if value.is_some() && ty.is_some() {
                return Some((value, ty));
            }
            // The opening it is written in (`parent_ctx`).
            c = ast.decls.lexical_ctx(x).or_else(|| ast.decls.get(x).ctx());
        } else {
            c = ast.decls.get(x).ctx();
        }
    }
    s.indeterminate(CHAIN_LIMIT);
    Some((value, ty))
}

/// The visibility word of the first attribute of `kind` on declaration
/// `x` itself (not its redeclarations).
fn attr_of(w: &Walk<'_>, x: DeclId, kind: AttrKind) -> Option<u32> {
    let ast = w.ast;
    ast.lists
        .iter::<AttrId>(ast.attrs.of_decl(x))
        .map(|a| ast.attrs.get(a))
        .find(|a| a.kind == kind)
        .map(AttrNode::visibility)
}

#[cfg(test)]
mod tests {
    use slip_kiln::kiln::ast::decl::DeclKind;

    use crate::decl_func::tests::{named, with_walk};
    use crate::stream::Stream;
    use crate::tables::{Row, linkage, walk_row};

    /// The row's head: its tag, the linkage and whether an unnamed
    /// namespace encloses the entity.
    fn head(link: Row, unnamed: bool) -> Vec<u8> {
        let mut s = Stream::new();
        s.uv(u64::from(walk_row::ENCLOSING)).tag(link).bool(unnamed);
        s.as_bytes().to_vec()
    }

    /// The row written for the last declaration named `name` of `kind`.
    fn row(src: &str, name: &str, kind: DeclKind) -> Vec<u8> {
        with_walk(src, |w| {
            let d = *named(w, name, kind).last().expect("the declaration");
            let mut s = Stream::new();
            super::write(w, &mut s, d);
            s.as_bytes().to_vec()
        })
    }

    /// Kiln WP-2623/2624 (`kiln_ast/src/linkage.rs`
    /// `extern_c_escapes_unnamed_namespace`): a function or variable first
    /// declared in an `extern "C"` context keeps external linkage in an
    /// unnamed namespace, as in clang (internal at `913a490bb`), and so
    /// the row is the one place `Ast::linkage` and the unnamed namespace
    /// part; a block-scope `extern` declaration in a function of an
    /// unnamed namespace is internal (external at `913a490bb`).
    #[test]
    fn wp066_linkage_of_unnamed_namespace_members() {
        let cases: [(&str, &str, DeclKind, Row, bool); 5] = [
            (
                "namespace { extern \"C\" int f(); }",
                "f",
                DeclKind::Function,
                linkage::EXTERNAL,
                true,
            ),
            ("namespace { extern \"C\" int v; }", "v", DeclKind::Var, linkage::EXTERNAL, true),
            ("namespace { int f(); }", "f", DeclKind::Function, linkage::INTERNAL, true),
            (
                "namespace { void g() { extern int v; } }",
                "v",
                DeclKind::Var,
                linkage::INTERNAL,
                true,
            ),
            (
                "namespace n { void g() { extern int v; } }",
                "v",
                DeclKind::Var,
                linkage::EXTERNAL,
                false,
            ),
        ];
        for (src, name, kind, link, unnamed) in cases {
            let got = row(src, name, kind);
            assert!(got.starts_with(&head(link, unnamed)), "{src}: {got:?}");
        }
    }
}
