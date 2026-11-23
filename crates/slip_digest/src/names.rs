//! `Output::names`: each unit's entity as a person would write it
//! (`verify.md` §6 `name`). For messages and the pair tests only: never
//! hashed.
//!
//! The forms (every one is a row of `spec/verify.md` §6.1):
//!
//! - a qualified name (`ns::C::m`, `(anonymous namespace)::x`), with a
//!   specialisation's arguments (`ns::S<int>`), a null pointer argument
//!   of a type other than `std::nullptr_t` as `(T)nullptr`;
//! - a function with its parameter types, as Kiln prints canonical types
//!   (`ns::f(int, char *)`), `this ` before an explicit object parameter,
//!   and ` const`, ` volatile`, ` &`, ` &&` after them for a method's
//!   qualifiers, then ` requires ` and a trailing requires-clause; a
//!   deduction guide likewise;
//! - a function template with its head and, but for a constructor, a
//!   destructor and a conversion function, its return type before the
//!   name (`template <class, int> void ns::f(type-parameter-0-0)`): both
//!   are in its key (digest-kinds §3.3);
//! - an unnamed class or enumeration as the name that keys it (digest
//!   §3.3): `(anonymous struct for P)`, `(anonymous enum 1)` for the
//!   second of its scope named by ordinal; its members under that name;
//! - a name-independent member with its ordinal, `S::_ (name-independent 1)`;
//! - an unnamed bit-field as `(unnamed N)`, the unnamed object of an
//!   anonymous class as `(object of (anonymous union 0))`;
//! - the variable of a structured binding declaration as its bindings,
//!   `[a, b]` (its key is named by `a` alone), and the holding variable of a tuple-like binding `a` as
//!   `a (holding variable)`;
//! - a using-declaration as `<using Q::n>` and a using-shadow as
//!   `<shadow T>` (`T` the name of its target), under their scope;
//! - a friend function keyed in the class that befriends it (digest-kinds
//!   §3.3 `function`) as `A<int>::friend k(int)`;
//! - a content-keyed unit as its scope and `<kind>`, with what it names
//!   for a friend (`<friend C>`), a using-directive and a using-enum
//!   (`ns::<using_directive n>`);
//! - a resolved unit as its owner's name and `::<property>`, with its
//!   discriminator after a space (`S::<key_function>`,
//!   `f(int)::<default_immediates 0>`); a `default_argument` unit as its
//!   function's or field's name and `::<default_argument n>`
//!   (`X<int>::f(int)::<default_argument 0>`), before the property of a
//!   resolved unit of it;
//! - under a local type, the type as its key, its owner's name and
//!   `local(n)` (`mk()::local(0)::operator()<int>(int) const`, `?` for a
//!   number not known; `spec/landed/digest/WP-040.md`);
//! - the order unit as `<order>`.

use std::collections::BTreeMap;

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::RawId;
use slip_kiln::kiln::ast::consts::Scalar;
use slip_kiln::kiln::ast::decl::{DeclKind, FriendTarget};
use slip_kiln::kiln::ast::dump::AstNames;
use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::ids::{ArgListId, ConstValueId, ExprId, QualType};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::templates::TtpKind;
use slip_kiln::kiln::ast::types::{BuiltinType, RefQual, TypeNames, TypeView};
use slip_kiln::kiln::ast::types::{
    PrintOptions, print_type, write_arg_list, write_nns, write_template_arg,
};
use slip_kiln::kiln::source::symbol::Symbol;
use slip_model::Digest;
use slip_model::keys::KeyKind;

use crate::keys::{Role, info};
use crate::units::Written;
use crate::walk::Walk;

/// How deep a name nests unnamed classes or friends before it stops
/// (an AST is far shallower).
const DEPTH: u32 = 64;

/// Every unit's name, sorted by key (and by name for units sharing one).
pub(crate) fn names(w: &Walk<'_>, written: &[Written]) -> Vec<(Digest, String)> {
    let n = Names::new(w);
    let mut out: Vec<(Digest, String)> = written
        .iter()
        .map(|u| {
            let name = match u.entity {
                Some(e) => {
                    let mut name = n.unit(u.read.unwrap_or(e), e, u.kind);
                    if let Some(i) = u.default_argument {
                        name.push_str("::<default_argument ");
                        name.push_str(&i.to_string());
                        name.push('>');
                    }
                    if let Some(r) = u.resolved {
                        name.push_str("::<");
                        name.push_str(r.property.word());
                        if let Some(i) = r.disc {
                            name.push(' ');
                            name.push_str(&i.to_string());
                        }
                        name.push('>');
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

/// The name of the entity `e` (a declaration that is its entity, as
/// `Walk::entity` gives it) for a person: the host's `display_string_of`
/// (`spec/api.md` §6.3 rule 2, `docs/wp/WP-083.md`). Never hashed. It is
/// the qualified form without what needs the walk: `Output::names` adds
/// a function template's head and return type, the names of unnamed
/// classes and the content-keyed forms (`spec/verify.md` §6.1). An entity Kiln prints without a
/// name ends in `(unnamed)` here, since the ordinal of digest §3.3 is the
/// walk's.
pub fn display_name(ast: &Ast, e: DeclId) -> String {
    let kind = match info(ast.decls.get(e).kind()).role {
        Role::Entity(k) | Role::Content(k) => k,
        // Any kind other than `FunctionTemplate` names it the same way:
        // `entity_name` reads `kind` only to tell a function template.
        _ => KeyKind::Variable,
    };
    let mut name = entity_name(ast, e, kind);
    if name.is_empty() || name.ends_with("::") {
        name.push_str("(unnamed)");
    }
    name
}

/// The name of entity `e` of key kind `kind`, read from `e`: the
/// qualified form, without what needs the walk (the names of unnamed
/// classes).
pub(crate) fn entity_name(ast: &Ast, e: DeclId, kind: KeyKind) -> String {
    let names = AstNames::new(ast);
    let mut out = String::new();
    names.write_qualified_spec_name(e, &mut out);
    spec_args(ast, e, &mut out);
    if kind != KeyKind::FunctionTemplate && info(ast.decls.get(e).kind()).function {
        function_suffix(ast, e, &mut out);
    }
    out
}

/// What naming needs besides the AST: the walk (how unnamed classes and
/// friends are keyed) and the holding variables of tuple-like bindings.
struct Names<'n, 'w> {
    w: &'n Walk<'w>,
    ast: &'n Ast,
    kiln: AstNames<'n>,
    /// Holding variable to its binding.
    holding: BTreeMap<DeclId, DeclId>,
}

impl<'n, 'w> Names<'n, 'w> {
    fn new(w: &'n Walk<'w>) -> Self {
        let ast = w.ast;
        let holding = ast.decls.cold.binding_var.iter().map(|(b, &h)| (h, b)).collect();
        Names { w, ast, kiln: AstNames::new(ast), holding }
    }

    /// The name of the unit of entity `e`, read from declaration `d`.
    fn unit(&self, d: DeclId, e: DeclId, kind: KeyKind) -> String {
        let ast = self.ast;
        let mut out = String::new();
        let n = ast.decls.get(d);
        match info(n.kind()).role {
            Role::Content(_) => {
                self.scope(d, &mut out, 0);
                out.push('<');
                out.push_str(content_word(kind));
                self.content_object(d, &mut out);
                out.push('>');
                return out;
            }
            Role::Entity(_)
            | Role::Nothing
            | Role::Transparent
            | Role::Root
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => {}
        }
        match kind {
            KeyKind::UsingDeclaration => {
                self.scope(d, &mut out, 0);
                out.push_str("<using ");
                let mut q = String::new();
                if let Some(nns) = ast.using_qualifier(d) {
                    write_nns(&ast.types, ast, nns, &mut q);
                }
                out.push_str(&q);
                // An inheriting constructor's name is the derived class's
                // constructor in Kiln: written as `B::B`, the qualifier's
                // last name.
                if let DeclNameView::Constructor(_) = ast.names.view(n.name()) {
                    let last = q.trim_end_matches("::").rsplit("::").next().unwrap_or_default();
                    out.push_str(last.split('<').next().unwrap_or_default());
                } else {
                    self.kiln.write_name(d, &mut out);
                }
                out.push('>');
                return out;
            }
            KeyKind::UsingShadow => {
                self.scope(d, &mut out, 0);
                out.push_str("<shadow ");
                match ast.using_target(d) {
                    Some(t) => out.push_str(&self.plain(t)),
                    None => self.kiln.write_name(d, &mut out),
                }
                out.push('>');
                return out;
            }
            KeyKind::Decomposition => {
                self.scope(d, &mut out, 0);
                self.kiln.write_diag_name(d, &mut out);
                return out;
            }
            KeyKind::FunctionTemplate => {
                if let Some(class) = self.w.friend_class(e) {
                    out.push_str(&self.unit(class, self.w.entity(class), KeyKind::Class));
                    out.push_str("::friend ");
                }
                self.function_template(d, &mut out);
                return out;
            }
            KeyKind::Function => {
                if let Some(class) = self.w.friend_class(e) {
                    out.push_str(&self.unit(class, self.w.entity(class), KeyKind::Class));
                    out.push_str("::friend ");
                }
            }
            KeyKind::PartialSpecialisation => {
                // The head is in the key (digest-kinds §3.3); what the
                // arguments do not show of it is its constraints.
                if let Some((_, pi)) = ast.partial_spec_info(d)
                    && self.constrained(pi.params)
                {
                    self.head(pi.params, &mut out, 0);
                    out.push(' ');
                }
            }
            _ => {}
        }
        self.qualified(d, &mut out, 0);
        spec_args(ast, d, &mut out);
        self.suffix(d, e, kind, &mut out);
        out
    }

    /// Whether head `l` has a requires-clause or a constrained parameter.
    fn constrained(&self, l: slip_kiln::kiln::ast::TemplateParamListId) -> bool {
        let ast = self.ast;
        let list = ast.templates.param_list(l);
        list.requires.is_some()
            || ast
                .lists
                .iter::<DeclId>(list.params)
                .any(|p| ast.template_parm(p).is_some_and(|x| x.constraint.is_some()))
    }

    /// What follows the qualified name of the unit of entity `e`, read from
    /// declaration `d`: the parameters, and the marks that tell apart units
    /// that would otherwise share a name.
    fn suffix(&self, d: DeclId, e: DeclId, kind: KeyKind, out: &mut String) {
        let ast = self.ast;
        let node = ast.decls.get(d);
        let kind_of = node.kind();
        if info(kind_of).function || kind_of == DeclKind::CXXDeductionGuide {
            function_suffix(ast, d, out);
        }
        if self.holding.contains_key(&e) || self.holding.contains_key(&d) {
            out.push_str(" (holding variable)");
        }
        // Two name-independent members of one name: their ordinal, as in
        // the key (`keys.rs` `name_independent_ordinal`).
        if kind == KeyKind::Field
            && node.misc().name_independent()
            && let Some(ordinal) = self.w.name_independent_ordinal(d)
        {
            out.push_str(" (name-independent ");
            out.push_str(&ordinal.to_string());
            out.push(')');
        }
        // An unnamed field or variable: the object of an anonymous class,
        // named by it, or a bit-field, by its ordinal (digest §3.3).
        if out.is_empty() || out.ends_with("::") {
            let anonymous = node.ty().and_then(|q| ast.types.as_record(ast.types.canonical(q)));
            match (anonymous, self.w.ordinal_of.get(&e)) {
                (Some(r), _) if self.is_unnamed_tag(r) => {
                    out.push_str("(object of ");
                    self.unnamed_tag(r, out);
                    out.push(')');
                }
                (_, Some(i)) => {
                    out.push_str("(unnamed ");
                    out.push_str(&i.to_string());
                    out.push(')');
                }
                (_, None) => out.push_str("(unnamed)"),
            }
        }
    }

    /// The name of an entity a unit refers to (a using-shadow's target,
    /// what a friend names): qualified, with its arguments and a
    /// function's signature.
    fn plain(&self, d: DeclId) -> String {
        let ast = self.ast;
        let mut out = String::new();
        if ast.decls.get(d).kind() == DeclKind::FunctionTemplate {
            self.function_template(d, &mut out);
            return out;
        }
        self.qualified(d, &mut out, 0);
        spec_args(ast, d, &mut out);
        let k = ast.decls.get(d).kind();
        if info(k).function || k == DeclKind::CXXDeductionGuide {
            function_suffix(ast, d, &mut out);
        }
        out
    }

    /// What a content-keyed unit names, after its word: ` <target>` for
    /// a friend, a using-directive and a using-enum.
    fn content_object(&self, d: DeclId, out: &mut String) {
        let ast = self.ast;
        let k = ast.decls.get(d).kind();
        if k == DeclKind::Friend {
            out.push(' ');
            match ast.decls.friend_target(d) {
                FriendTarget::Decl(x) => out.push_str(&self.plain(x)),
                FriendTarget::Type { ty, owned_tag: _ } => {
                    out.push_str(&print_type(&ast.types, ast, ty, PrintOptions::CANONICAL));
                }
            }
        } else if (k == DeclKind::UsingDirective || k == DeclKind::UsingEnum)
            && let Some(t) = ast.using_target(d)
        {
            out.push(' ');
            out.push_str(&self.plain(t));
        }
    }

    /// `template <…> R name(params)` of function template `t`.
    fn function_template(&self, t: DeclId, out: &mut String) {
        let ast = self.ast;
        let Some(i) = ast.template_info(t).copied() else {
            self.qualified(t, out, 0);
            return;
        };
        self.head(i.params, out, 0);
        out.push(' ');
        let f = i.pattern.unwrap_or(t);
        let shows_return = !matches!(
            ast.decls.get(f).kind(),
            DeclKind::CXXConstructor | DeclKind::CXXDestructor | DeclKind::CXXConversion
        );
        if shows_return
            && let Some(q) = ast.decls.get(f).ty()
            && let Some(p) = ast.types.proto(q)
        {
            out.push_str(&print_type(&ast.types, ast, p.ret(), PrintOptions::CANONICAL));
            out.push(' ');
        }
        self.qualified(t, out, 0);
        function_suffix(ast, f, out);
    }

    /// `template <class, int, template <class> class>`: a template head
    /// by the kinds of its parameters (their names are not the entity's).
    fn head(&self, l: slip_kiln::kiln::ast::TemplateParamListId, out: &mut String, depth: u32) {
        let ast = self.ast;
        out.push_str("template <");
        let list = ast.templates.param_list(l);
        for (k, p) in ast.lists.iter::<DeclId>(list.params).enumerate() {
            if k > 0 {
                out.push_str(", ");
            }
            let parm = ast.template_parm(p).copied();
            let k = ast.decls.get(p).kind();
            if k == DeclKind::NonTypeTemplateParm {
                match ast.decls.get(p).ty() {
                    Some(q) => {
                        out.push_str(&print_type(&ast.types, ast, q, PrintOptions::CANONICAL));
                    }
                    None => out.push_str("auto"),
                }
            } else if k == DeclKind::TemplateTemplateParm {
                match parm.and_then(|x| x.params) {
                    Some(inner) if depth < DEPTH => self.head(inner, out, depth + 1),
                    Some(_) | None => out.push_str("template <>"),
                }
                out.push_str(match parm.map(|x| x.ttp_kind) {
                    Some(TtpKind::Variable) => " auto",
                    Some(TtpKind::Concept) => " concept",
                    Some(TtpKind::Type) | None => " class",
                });
            } else if let Some(c) = parm.and_then(|x| x.constraint) {
                self.type_constraint(c, out);
            } else {
                out.push_str("class");
            }
            if parm.is_some_and(|x| x.pack) {
                out.push_str("...");
            }
        }
        out.push('>');
        if let Some(r) = list.requires {
            out.push_str(" requires ");
            ast.write_expr(r, out);
        }
    }

    /// A type parameter's type-constraint, from its immediately-declared
    /// constraint `C<T, A...>` (a fold `(C<T> && ...)` for a pack): the
    /// concept and the arguments after the first, `C`, `C<int>`, as it is
    /// written in place of `class`. Anything else is written as Kiln
    /// writes the expression.
    fn type_constraint(&self, c: ExprId, out: &mut String) {
        let ast = self.ast;
        let mut x = ast.exprs.get(c);
        if x.kind() == ExprKind::CXXFoldExpr
            && let Some(inner) = ExprId::from_raw(x.a())
        {
            x = ast.exprs.get(inner);
        }
        let concept =
            DeclId::from_raw(x.a()).filter(|_| x.kind() == ExprKind::ConceptSpecializationExpr);
        let Some(concept) = concept else {
            ast.write_expr(c, out);
            return;
        };
        self.qualified(concept, out, 0);
        let Some(args) = ArgListId::from_raw(x.b()) else { return };
        let all = ast.types.args(args);
        if all.len() > 1 {
            out.push('<');
            for (i, a) in all.iter().skip(1).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_template_arg(&ast.types, &TypedNull(ast), *a, out);
            }
            out.push('>');
        }
    }

    /// The qualified name of `d`, with the unnamed classes and
    /// enumerations among its scopes, and `d` itself if it is one, named
    /// as their keys name them.
    fn qualified(&self, d: DeclId, out: &mut String, depth: u32) {
        let mut kiln = String::new();
        self.kiln.write_qualified_spec_name(d, &mut kiln);
        if depth < DEPTH
            && let Some(rest) = self.local_scope(d, &kiln)
        {
            out.push_str(&rest);
            return;
        }
        if depth < DEPTH && self.is_unnamed_tag(d) {
            // Kiln's last component is `(anonymous struct)` or the typedef
            // name; the scope before it is that of `d`.
            self.scope(d, out, depth);
            self.unnamed_tag(d, out);
            return;
        }
        if depth < DEPTH
            && let Some(t) = self.unnamed_scope(d)
        {
            let mut prefix = String::new();
            self.kiln.write_qualified_spec_name(t, &mut prefix);
            prefix.push_str("::");
            if let Some(rest) = kiln.strip_prefix(&prefix) {
                self.qualified(t, out, depth + 1);
                out.push_str("::");
                out.push_str(rest);
                return;
            }
        }
        out.push_str(&kiln);
    }

    /// The scope of `d` followed by `::` (empty at the top level), as
    /// [`Self::qualified`] names it.
    fn scope(&self, d: DeclId, out: &mut String, depth: u32) {
        let ast = self.ast;
        let Some(ctx) = ast.decls.get(d).ctx() else { return };
        let k = ast.decls.get(ctx).kind();
        if k == DeclKind::TranslationUnit {
            return;
        }
        if depth < DEPTH && self.local_type_around(d).is_some() {
            let mut kiln = String::new();
            self.kiln.write_qualified_spec_name(ctx, &mut kiln);
            if let Some(rest) = self.local_scope(ctx, &kiln) {
                out.push_str(&rest);
                out.push_str("::");
                return;
            }
        }
        if depth < DEPTH && (self.is_unnamed_tag(ctx) || self.unnamed_scope(ctx).is_some()) {
            self.qualified(ctx, out, depth + 1);
            out.push_str("::");
            return;
        }
        self.kiln.write_scope_prefix(d, true, out);
    }

    /// The nearest local type around `d` (digest §4.3), through the
    /// classes nested in it; `None` outside one.
    fn local_type_around(&self, d: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let mut c = ast.decls.get(d).ctx();
        for _ in 0..DEPTH {
            let x = c?;
            if crate::local::is_local_type(self.w, x) {
                return Some(x);
            }
            if !self.w.is_local(x) {
                return None;
            }
            c = ast.decls.get(x).ctx();
        }
        None
    }

    /// `kiln`, Kiln's qualified name of `d`, with the scope it prints for
    /// the nearest local type around `d` replaced by that type's key: its
    /// owner's name and `local(n)` (`?` for a number not known), then the
    /// classes between it and `d` and `d`'s own component as Kiln prints
    /// them. `None` outside a local type.
    #[allow(
        clippy::many_single_char_names,
        reason = "the names of digest-kinds: s stream, d declaration, e entity, n number, o owner, t type"
    )]
    fn local_scope(&self, d: DeclId, kiln: &str) -> Option<String> {
        let ast = self.ast;
        let t = self.local_type_around(d)?;
        // Kiln prints a closure as `(lambda)` in a component, as
        // `(anonymous class)` in a scope: cut at the scope it prints for
        // `d`'s nearest local type, whatever it calls it.
        let mut scope_of_d = String::new();
        self.kiln.write_scope_prefix(d, true, &mut scope_of_d);
        let rest = kiln.strip_prefix(&scope_of_d)?;
        let mut inner = String::new();
        let mut x = ast.decls.get(d).ctx();
        while let Some(c) = x {
            if c == t {
                break;
            }
            let mut one = String::new();
            self.kiln.write_name(c, &mut one);
            inner = format!("{one}::{inner}");
            x = ast.decls.get(c).ctx();
        }
        let (owner, n) = match crate::local::known_number(self.w, t) {
            Some((o, n)) => (o, n.to_string()),
            None => (crate::local::owner(self.w, t)?, "?".to_owned()),
        };
        let kind = match info(ast.decls.get(owner).kind()).role {
            Role::Entity(k) | Role::Content(k) => k,
            Role::Nothing
            | Role::Transparent
            | Role::Root
            | Role::Local
            | Role::InParent
            | Role::Unsupported(_) => KeyKind::Function,
        };
        let owner_name = self.unit(owner, self.w.entity(owner), kind);
        Some(format!("{owner_name}::local({n})::{inner}{rest}"))
    }

    /// `d` is a class or enumeration without a name of its own.
    fn is_unnamed_tag(&self, d: DeclId) -> bool {
        let ast = self.ast;
        let k = ast.decls.get(d).kind();
        matches!(k, DeclKind::CXXRecord | DeclKind::Enum)
            && ast.decls.get(d).name().is_empty()
            && ast.decls.record(d).is_none_or(|r| r.lambda.is_none())
    }

    /// The innermost unnamed class around `d` (an unnamed enumeration
    /// is no scope Kiln prints).
    fn unnamed_scope(&self, d: DeclId) -> Option<DeclId> {
        let ast = self.ast;
        let mut c = ast.decls.get(d).ctx();
        for _ in 0..DEPTH {
            let x = c?;
            let k = ast.decls.get(x).kind();
            if k == DeclKind::TranslationUnit {
                return None;
            }
            if k == DeclKind::CXXRecord && self.is_unnamed_tag(x) {
                return Some(x);
            }
            c = ast.decls.get(x).ctx();
        }
        None
    }

    /// `(anonymous struct for P)`, `(anonymous enum 1)`: unnamed tag `d`
    /// by the rule of digest §3.3 its key follows.
    fn unnamed_tag(&self, d: DeclId, out: &mut String) {
        let ast = self.ast;
        let e = self.w.entity(d);
        out.push_str("(anonymous ");
        out.push_str(match ast.decls.record(e) {
            Some(r) => r.tag.keyword(),
            None => "enum",
        });
        if let Some(by) = self.w.unnamed_tag_by(e) {
            out.push_str(" for ");
            self.kiln.write_name(by, out);
        } else if let Some(i) = self.w.ordinal_of.get(&e) {
            out.push(' ');
            out.push_str(&i.to_string());
        }
        out.push(')');
    }
}

/// A specialisation's arguments, or a partial specialisation's pattern.
fn spec_args(ast: &Ast, d: DeclId, out: &mut String) {
    let names = TypedNull(ast);
    if let Some((_, si)) = ast.spec_info(d) {
        write_arg_list(&ast.types, &names, ast.types.canonical_args(si.args), out);
    } else if let Some((_, pi)) = ast.partial_spec_info(d) {
        write_arg_list(&ast.types, &names, pi.canonical, out);
    }
}

/// The AST's names, except that a null pointer argument whose type is not
/// `std::nullptr_t` prints with it (`(int *)nullptr`): Kiln's argument
/// printer writes every null pointer `nullptr`, and two such arguments of
/// different types are different specialisations (`N<nullptr>` against
/// `N<(int *)nullptr>`).
struct TypedNull<'a>(&'a Ast);

impl TypedNull<'_> {
    /// Writes `value` as a typed null pointer, if it is one whose type is
    /// not `std::nullptr_t`.
    fn typed_null(&self, value: ConstValueId, out: &mut String) -> bool {
        let ast = self.0;
        let Some(Scalar::Null { ty, kind: _ }) = ast.consts.scalar(value) else { return false };
        let c = ast.types.canonical(ty);
        if let TypeView::Builtin(BuiltinType::NullPtr) = ast.types.view(c.ty()) {
            return false;
        }
        out.push('(');
        out.push_str(&print_type(&ast.types, ast, c, PrintOptions::CANONICAL));
        out.push_str(")nullptr");
        true
    }
}

impl TypeNames for TypedNull<'_> {
    fn write_decl_name(&self, decl: DeclId, scoped: bool, out: &mut String) {
        self.0.write_decl_name(decl, scoped, out);
    }
    fn specialization_args(&self, decl: DeclId) -> Option<ArgListId> {
        self.0.specialization_args(decl)
    }
    fn args_as_written(&self, decl: DeclId) -> Option<ArgListId> {
        self.0.args_as_written(decl)
    }
    fn converted_args(&self, decl: DeclId) -> Option<ArgListId> {
        self.0.converted_args(decl)
    }
    fn write_converted_scope(&self, decl: DeclId, out: &mut String) -> bool {
        self.0.write_converted_scope(decl, out)
    }
    fn implicit_typedef(&self, decl: DeclId) -> bool {
        self.0.implicit_typedef(decl)
    }
    fn preferred_name(&self, decl: DeclId) -> Option<QualType> {
        self.0.preferred_name(decl)
    }
    fn tag_keyword(&self, decl: DeclId) -> &'static str {
        self.0.tag_keyword(decl)
    }
    fn tag_named_by_typedef(&self, decl: DeclId) -> bool {
        self.0.tag_named_by_typedef(decl)
    }
    fn invented_parm(&self, decl: DeclId) -> Option<Option<(DeclId, ArgListId)>> {
        self.0.invented_parm(decl)
    }
    fn write_expr(&self, expr: ExprId, out: &mut String) {
        self.0.write_expr(expr, out);
    }
    fn write_value(&self, ty: QualType, value: ConstValueId, out: &mut String) {
        if !self.typed_null(value, out) {
            self.0.write_value(ty, value, out);
        }
    }
    fn write_value_typed(&self, ty: QualType, value: ConstValueId, out: &mut String) {
        if !self.typed_null(value, out) {
            self.0.write_value_typed(ty, value, out);
        }
    }
    fn arg_includes_type(&self, template: DeclId, index: usize) -> bool {
        self.0.arg_includes_type(template, index)
    }
    fn write_symbol(&self, sym: Symbol, out: &mut String) {
        self.0.write_symbol(sym, out);
    }
    fn write_template_param(&self, associated: DeclId, index: u32, out: &mut String) {
        self.0.write_template_param(associated, index, out);
    }
    fn write_operator(&self, op: u8, out: &mut String) {
        self.0.write_operator(op, out);
    }
    fn is_anonymous_namespace(&self, ns: DeclId) -> bool {
        self.0.is_anonymous_namespace(ns)
    }
    fn clang_compat(&self) -> u32 {
        self.0.clang_compat()
    }
    fn c_lang(&self) -> Option<bool> {
        self.0.c_lang()
    }
}

/// `(T1, T2)` and the method qualifiers of function `f`.
fn function_suffix(ast: &Ast, f: DeclId, out: &mut String) {
    let Some(q) = ast.decls.get(f).ty() else { return };
    let c = ast.types.canonical(q);
    let TypeView::Function(p) = ast.types.view(c.ty()) else { return };
    out.push('(');
    // The explicit object parameter (P0847), which the type alone does not
    // tell from an ordinary first parameter (the key's explicit-object
    // flag, digest-kinds §3.3 `function`).
    let explicit = ast.decls.function(f).is_some_and(|d| d.spec.has_explicit_object());
    for (i, &t) in ast.types.params(&p).iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        } else if explicit {
            out.push_str("this ");
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
    // The trailing requires-clause, which the key holds (digest-kinds §3.3
    // `function`): two functions that differ in it alone are two units.
    if let Some(&r) = ast.templates.trailing_requires.get(f) {
        out.push_str(" requires ");
        ast.write_expr(r, out);
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
