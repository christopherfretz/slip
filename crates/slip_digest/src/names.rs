//! `Output::names`: each unit's entity as a person would write it
//! (`verify.md` §6 `name`). For messages and the pair tests only: never
//! hashed.
//!
//! The forms: a qualified name (`ns::C::m`, `(anonymous namespace)::x`);
//! a function with its parameter types, as Kiln prints canonical types
//! (`ns::f(int, char *)`), and ` const`, ` volatile`, ` &`, ` &&` after
//! them for a method's qualifiers; a specialisation with its arguments
//! (`ns::S<int>`); a content-keyed unit as its scope and `<kind>`
//! (`ns::<static_assert>`); the order unit as `<order>`.

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::dump::AstNames;
use slip_kiln::kiln::ast::types::{PrintOptions, print_type, write_arg_list};
use slip_kiln::kiln::ast::types::{RefQual, TypeView};
use slip_model::Digest;
use slip_model::keys::KeyKind;

use crate::keys::{Role, info};
use crate::units::Written;
use crate::walk::Walk;

/// Every unit's name, sorted by key (and by name for units sharing one).
pub(crate) fn names(w: &Walk<'_>, written: &[Written]) -> Vec<(Digest, String)> {
    let mut out: Vec<(Digest, String)> = written
        .iter()
        .map(|u| {
            let name = match u.entity {
                Some(e) => {
                    let mut name = entity_name(w.ast, e, u.kind);
                    // An entity Kiln prints without a name: its ordinal
                    // (digest §3.3) when it has one.
                    if name.is_empty() || name.ends_with("::") {
                        match w.ordinal_of.get(&e) {
                            Some(n) => {
                                name.push_str("(unnamed ");
                                name.push_str(&n.to_string());
                                name.push(')');
                            }
                            None => name.push_str("(unnamed)"),
                        }
                    }
                    name
                }
                None => "<order>".to_owned(),
            };
            (u.key, name)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The name of entity `e` of key kind `kind`.
pub(crate) fn entity_name(ast: &Ast, e: DeclId, kind: KeyKind) -> String {
    let names = AstNames::new(ast);
    let mut out = String::new();
    let n = ast.decls.get(e);
    if let Role::Content(_) = info(n.kind()).role {
        if let Some(ctx) = n.ctx()
            && info(ast.decls.get(ctx).kind()).role != Role::Root
        {
            names.write_qualified_name(ctx, &mut out);
            out.push_str("::");
        }
        out.push('<');
        out.push_str(content_word(kind));
        out.push('>');
        return out;
    }
    names.write_qualified_spec_name(e, &mut out);
    if let Some((_, si)) = ast.spec_info(e) {
        write_arg_list(&ast.types, ast, ast.types.canonical_args(si.args), &mut out);
    } else if let Some((_, pi)) = ast.partial_spec_info(e) {
        write_arg_list(&ast.types, ast, pi.canonical, &mut out);
    }
    let function = match ast.template_info(e).and_then(|i| i.pattern) {
        Some(p) if kind == KeyKind::FunctionTemplate => Some(p),
        Some(_) | None => info(n.kind()).function.then_some(e),
    };
    if let Some(f) = function {
        function_suffix(ast, f, &mut out);
    }
    out
}

/// `(T1, T2)` and the method qualifiers of function `f`.
fn function_suffix(ast: &Ast, f: DeclId, out: &mut String) {
    let Some(q) = ast.decls.get(f).ty() else { return };
    let c = ast.types.canonical(q);
    let TypeView::Function(p) = ast.types.view(c.ty()) else { return };
    out.push('(');
    for (i, &t) in ast.types.params(&p).iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&print_type(&ast.types, ast, t, PrintOptions::CANONICAL));
    }
    if p.info().variadic() {
        if !ast.types.params(&p).is_empty() {
            out.push_str(", ");
        }
        out.push_str("...");
    }
    out.push(')');
    let fi = p.info();
    let tq = fi.this_quals();
    if tq.is_const() {
        out.push_str(" const");
    }
    if tq.is_volatile() {
        out.push_str(" volatile");
    }
    match fi.ref_qual() {
        RefQual::None => {}
        RefQual::LValue => out.push_str(" &"),
        RefQual::RValue => out.push_str(" &&"),
    }
}

/// The word standing for a content-keyed unit.
fn content_word(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::FileScopeAsm => "asm",
        KeyKind::UsingDirective => "using_directive",
        KeyKind::UsingEnum => "using_enum",
        KeyKind::Friend => "friend",
        KeyKind::StaticAssert => "static_assert",
        KeyKind::ConstevalBlock => "consteval_block",
        KeyKind::TemplateParamObject => "template_param_object",
        _ => "content",
    }
}
