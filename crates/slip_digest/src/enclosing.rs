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
//!   which Kiln's mangler and code generation read for the symbol's
//!   linkage (`kiln_abi::mangle::has_internal_linkage`) where it differs
//!   from `Ast::linkage` (a block-scope `extern` declaration in a function
//!   of an unnamed namespace);
//! - for the first declaration and the definition, the visibility that the nearest enclosing
//!   namespace with a `visibility` (or, for a type's visibility, a
//!   `type_visibility`) attribute gives it, found as Kiln's code
//!   generation finds it (`kiln_codegen/src/visibility.rs`, `ns_lv` and
//!   `parent_ctx`): from the declaration's context, through each
//!   namespace's lexical context (the opening it is written in). The
//!   attribute is the opening's own, not the namespace's: one opening
//!   with it and one without give their members different visibility.
//!
//! An entity inside a class or a function takes all of these from that
//! class or function (`spec/landed/digest/WP-113.md`, "The chain"), whose
//! unit holds this row, and does not get it.

use slip_kiln::kiln::ast::attr::AttrKind;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::linkage::Linkage;
use slip_kiln::kiln::ast::{AttrId, DeclId};

use crate::Cause;
use crate::stream::Stream;
use crate::tables::{linkage, walk_row};
use crate::walk::Walk;

/// A visibility word that is not 0, 1 or 2 (`AttrNode` `VISIBILITY.a`).
const BAD_VISIBILITY: Cause = Cause::Unaccounted("enclosing/visibility");
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
    let rep = w.representative(e);
    if w.is_local_extern(rep) {
        return true;
    }
    match ast.semantic_parent(rep) {
        None => true,
        Some(c) => matches!(
            ast.decls.get(c).kind(),
            DeclKind::TranslationUnit | DeclKind::Namespace
        ),
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
    // Code generation reads the visibility of the first declaration for
    // some symbols and of the definition for others (`decl_visibility`
    // with `first` or the definition), so the first's is written, and the
    // definition's where it is another: a redeclaration that is neither
    // changes no symbol (`differ/_/key-extern-c-in-two-namespaces`), and a
    // definition whose openings give what the first's give says nothing
    // more (`equal/_/var-extern-before-definition`).
    let first = ast.first(rep);
    let at_first = namespace_visibility(w, s, first);
    // None for the definition class (§11.9), whose definition Kiln makes
    // on use.
    let def = if crate::decl_func::definition_class(w, e) {
        None
    } else {
        crate::units::defining_decl(w, e)
    };
    let at_def = def.map(|x| namespace_visibility(w, s, x));
    visibility(s, at_first.0);
    visibility(s, at_first.1);
    match at_def {
        Some(v) if v != at_first => {
            s.opt(true);
            visibility(s, v.0);
            visibility(s, v.1);
        }
        Some(_) | None => {
            s.opt(false);
        }
    }
}

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
/// gives it (Kiln's `vis_of` on a namespace: `type_visibility` first).
///
/// The walk starts at the opening `d` is written in (its lexical
/// context), as clang's does: an explicit specialisation written in a
/// hidden opening of a namespace whose first opening is plain is hidden
/// (`spec/landed/digest/WP-113.md`, "For Kiln"; Kiln's `ns_lv` starts at
/// the semantic context). Where no namespace is on the lexical path (a
/// qualified definition written at file scope), it starts at the semantic
/// context.
fn namespace_visibility(w: &Walk<'_>, s: &mut Stream, d: DeclId) -> (Option<u32>, Option<u32>) {
    let ast = w.ast;
    let node = ast.decls.get(d);
    if let Some(found) = walk(w, s, ast.decls.lexical_ctx(d).or_else(|| node.ctx())) {
        return found;
    }
    walk(w, s, node.ctx()).unwrap_or((None, None))
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
        .map(|a| a.visibility())
}
