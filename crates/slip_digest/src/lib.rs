//! Entity keys and the semantic digest (`spec/digest.md`).
//!
//! The trusted base of Slip's correctness argument
//! (`docs/design/03-equivalence.md`): kept small, specified field by field
//! against Kiln's AST, and reviewed twice.
//!
//! [`digest`] walks one finished translation unit and returns its
//! `digests` section (`spec/digest.md` §11), the counters of §12, why each
//! indeterminate unit is one, and optionally a readable name per unit.
//!
//! # The shape of the walk (WP-024)
//!
//! One context, `walk::Walk`, owns everything a walk keeps: the memo of
//! type digests, the table of keys, the worklist of units and the state of
//! the unit being written (its local ordinals). One type,
//! `stream::Stream`, is the only way bytes reach a hash: it has the
//! primitives of `spec/model.md` §2 (`uv`, `sv`, `bool`, `bytes`,
//! `digest`, `count`), `opt` (the presence byte of `?`), `tag` (a variant
//! row's number), `flags` (a record's `bit` rows gathered by
//! `stream::FlagWord`, `spec/digest-kinds.md` §2.1), and `indeterminate`,
//! `absorb` and `pending` for causes. A cause other than `Pending`
//! replaces `Pending`; otherwise the first cause met is kept.
//!
//! The walk is driven by `walk::run`: every declaration of the arena is
//! classified (`units::unit_class`), the declared units and the
//! instantiations are queued, and writing a `Ref` to an implicit entity
//! queues it as a derived unit (`spec/digest.md` §2.2); each entity is
//! queued once. Units are then keyed, checked for collisions (§3.4) and
//! grouped (`groups`).
//!
//! # The functions other areas call
//!
//! Every function takes the walk and the stream it writes to, and writes
//! one encoding of `spec/digest-kinds.md` §1.3:
//!
//! | Function | Writes |
//! |---|---|
//! | `Walk::write_ref(s, d)` (`refs.rs`) | `ref`: a `DeclId` resolved to its entity (through a using-shadow), `Ref::template_param` for a template parameter, `Ref::local` for a local entity, else the mapped `KeyId`; queues a derived unit |
//! | `Walk::write_ref_opt(s, d)` | `ref` of an `Option<DeclId>`: `Ref::none` when absent |
//! | `Walk::write_refs_sorted(s, ds)` | `ref*sorted` |
//! | `Walk::key(d)` (`keys.rs`) | nothing: the entity's key, mapped, with the cause it carries |
//! | `Walk::write_key(s, d)` | `key`: the mapped `KeyId` as a `digest` |
//! | `Walk::write_name(s, n)` | `name`: a `DeclName` as the `Name` of §3.2 |
//! | `Walk::write_sym(s, sym)` | `sym`: a symbol's spelling |
//! | `Walk::write_type(s, q)` (`types.rs`) | `type`: the type digest of a `QualType` |
//! | `Walk::write_type_opt(s, q)` | `type?` |
//! | `Walk::write_types(s, qs)` | `type*` |
//! | `Walk::write_exc_spec(s, e)` | `enum ExcSpec` in normal form (§4.5) |
//! | `Walk::write_nns(s, n)` | `enum Nns`, canonical (§4.6) |
//! | `Walk::write_template_name(s, n)` | `enum TemplateName`, canonical |
//! | `Walk::write_args(s, l)` | `enum ArgView*` of a canonical list |
//! | `Walk::write_func_proto(s, p, exc)` | `record FuncProto` |
//! | `Walk::decl_node(s, d, e)` (`decl.rs`) | the rows of `DeclNode` (§5.3) of declaration `d` of entity `e`, with `DeclFlags` and `DeclMisc` merged over the entity's declarations |
//! | `Walk::decl_stream(s, d, e)` | a declaration's stream (§5.1): `decl_node`, then `decl_data::cold`, `attrs::decl_attrs`, `templ::decl_maps`, `other::module_rows` |
//! | `Walk::content_hash(e)` | the unit digest of a content-keyed entity, which is its discriminator |
//! | `templ::template_head(w, s, l)` | the template head of a key (§9, §3.3): `record TemplateParamList`, each parameter as its `DeclNode` rows |
//! | `templ::template_head_unit(w, s, l)` | `record TemplateParamList` as a unit writes it: each parameter's whole stream, defaults included |
//! | `templ::template_parm_info(w, s, d)`, `templ::template_info(w, s, d)` | `record TemplateParmInfo`, `record TemplateInfo` (the pattern written whole) |
//! | `local::ordinal(w, d)` | the ordinal of a local entity of the unit (digest §4.3), first met first |
//! | `demand::deferrable(w, f)` | whether a function is of the deferrable class (§11.2); `None` where the demand area decides |
//! | `expr::child(w, s, e)`, `expr::child_opt`, `expr::profile(w, s, p)`, `expr::profile_expr(w, s, e)`, `expr::requires_clause` | pending (`Area::Expr`) |
//! | `stmt::child(w, s, st)` | pending (`Area::Stmt`) |
//! | `value::value(w, s, v)`, `value::string(w, s, l)` (`value.rs`) | `value`: a constant in place by structure, a constant it points to by content digest (§8); `string`: a string literal by content (§10.3) |
//! | `local::write_decl(w, s, d)` | `decl` (§2.4): a local entity's ordinal and stream, a block-scope variable with linkage a `ref`, a block-scope function a `ref` and its own default arguments; a local type pending (`Area::Local`) |
//! | `decl_func::{function_data, var_data, param_data, let_data}(w, s, d)` | one `record T` of §5.4 each; a function's and a variable's language linkage (§5.6 `ScopeData`) |
//! | `decl_func::implicit_instantiation(w, e)` | nothing: whether the entity is an implicit instantiation (its own kind, else an enclosing one), the first rule of §11.9 |
//! | `decl_func::definition_class(w, e)` | nothing: whether the entity is of the §11.9 definition class (defined on use, implicit, or defaulted on its first declaration) |
//! | `decl_func::instantiated_on_use(w, e)` | nothing: whether a function or variable is an implicit instantiation or named by `extern template` (its own kind, else an enclosing one) |
//! | `decl_func::var_type(w, d, e)` | nothing: the type a variable's unit holds, an array of unknown bound completed by another declaration |
//! | `units::defining_decl(w, e)` | nothing: the defining declaration of a variable ([basic.def]) or Kiln's `definition` of another entity |
//! | `Walk::linkage_spec(d, all)` (`keys.rs`) | nothing: the innermost `LinkageSpec` around `d` and its language (`None` for an unknown byte), through namespaces only unless `all` |
//! | `decl_class::{typedef_data, enum_data, enumerator_data, record_data, field_data}(w, s, d)` | one `record T` of §5.5 each: pending (`Area::DeclData`) |
//! | `decl_scope::{friend_target, namespace_data, static_assert_data, file_scope_asm, cold}(w, s, d)` | one `record T` of §5.6 each, and the rows of `ColdDeclData` (§5.7): pending (`Area::DeclData`) |
//! | `templ::templ_link(w, s, l)` | `record TemplLink` (§9.1), which brings in `SpecInfo`, `MemberSpecInfo` or `PartialSpecInfo` (§9.4) |
//! | `templ::expr_maps(w, s, e)` | the `in` rows of `TemplateTables` keyed by `ExprId` (§9.5), each in its empty form when absent |
//! | `local::lambda_data(w, s, l)` | `record LambdaData` (§10.4): pending (`Area::Local`) |
//! | `fp::fp_override(s, o)` | `record FpOverride` (§6.1): pending (`Area::Stmt`) |
//! | `attrs::decl_attrs(w, s, d)` | pending (`Area::Attrs`) |
//! | `other::module_rows(w, s, d)` | pending (`Area::Other`) |
//! | `templ::decl_maps(w, s, d)` | the `in` rows of `TemplateTables` keyed by `DeclId` (§9.5), each in its empty form when absent |
//! | `templ::param_object(w, s, d)` | the value of a `TemplateParamObject` (`param_object_values`), at its `data` position |
//!
//! A key is computed with the unit's state put aside (`Walk::in_key`):
//! an area must not write a local ordinal from inside one (`write_ref`
//! makes it `Unaccounted("key/local-ref")`).
//!
//! A `Ref` to a local entity bumps `UnitState::local_uses`, so that a type
//! digest that holds one is memoised for the unit only (digest §4.2); an
//! area that writes such a reference without `write_ref` must do the
//! same.
//!
//! # What bounds the stack
//!
//! Every recursive step (a type, a key, an `Nns`, a template name, an
//! argument list, a template head, a declaration stream) enters one level
//! of `walk::DEPTH_LIMIT` (200); past it the stream is indeterminate with
//! `Unaccounted("walk/depth")`. The memo of type digests and keys makes
//! the depth reached that of one new nesting, not of the whole AST.
//!
//! # How to fill an area
//!
//! A pending function has its final signature and the body
//! `s.pending(Area::X)`: the unit becomes indeterminate with
//! `Cause::Pending(Area::X)` and the fixed placeholder `uv 0` keeps the
//! stream well formed. A pending function of `decl_func.rs`,
//! `decl_class.rs` and `decl_scope.rs` also writes, after the placeholder,
//! the statements and expressions its record owns (a *reach stub*), so
//! that statements and expressions can be reached and tested before the
//! payload records land. The package that fills an area replaces the bodies
//! in its module, writes the rows of its tables in table order with the
//! functions above, adds its pair tests (`tests/differ/<short>/<row>/` and
//! the other roots), removes the names from `tests/tables/untested.txt`
//! with `SLIP_BLESS=1 cargo test -p slip_digest --test tables`, and
//! removes nothing else. The last area removes `Area` and
//! `Cause::Pending`.
//!
//! What a dispatch over a Kiln enumeration must look like: every variant
//! listed, no `_` arm, no `..` over variants, a view destructured without
//! a rest pattern (`spec/digest.md` §13 obligation 2).

#![forbid(unsafe_code)]

use slip_kiln::kiln;
use slip_model::Digest;
use slip_model::digests::{GroupClass, GroupSum};

/// The tables of `spec/digest-kinds.md` as Rust, generated by
/// `tests/tables.rs` (WP-023).
#[rustfmt::skip]
pub(crate) mod tables;

mod attrs;
mod decl;
mod decl_class;
mod decl_func;
mod decl_scope;
mod demand;
mod expr;
mod fp;
mod groups;
mod keys;
mod local;
mod names;
mod other;
mod refs;
mod stmt;
mod stream;
mod templ;
mod types;
mod units;
mod value;
mod walk;

#[cfg(test)]
mod tests;

/// A finished translation unit (`spec/digest.md` §1 rule 4).
pub struct Input<'a> {
    pub ast: &'a kiln::ast::Ast,
    pub sm: &'a kiln::source::SourceManager,
    /// The subject path of a buffer (digest §11): the caller knows the
    /// roots. Empty for a buffer that is no file.
    pub subject_path: &'a dyn Fn(kiln::source::FileId) -> Vec<u8>,
}

impl std::fmt::Debug for Input<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input").finish_non_exhaustive()
    }
}

/// Which groups carry their unit lists (`spec/digest.md` §11).
#[derive(Clone, Copy, Debug)]
pub enum Detail<'a> {
    None,
    All,
    Groups(&'a [(GroupClass, Vec<u8>)]),
    /// digest §11: every group whose sum is not in the list, and every
    /// group with an indeterminate unit.
    Differing(&'a [GroupSum]),
}

/// How to walk.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    pub rules: &'a slip_model::keys::RuleTable,
    pub detail: Detail<'a>,
    /// Fill `Output::names`.
    pub names: bool,
}

/// What a walk produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub digests: slip_model::digests::Digests,
    /// digest §12, sorted by name; plus `digest.indeterminate.<cause>`.
    pub counters: Vec<slip_model::Counter>,
    /// Why each indeterminate unit is one (the first cause met), sorted
    /// by key.
    pub causes: Vec<(Digest, Cause)>,
    /// For every unit, the entity as a person would write it
    /// (`verify.md` §6 `name`); sorted by key; empty unless asked.
    pub names: Vec<(Digest, String)>,
}

/// Why a unit is indeterminate (`spec/digest.md` §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Cause {
    /// An invalid declaration, an error-recovery node, an error type.
    Error,
    /// A row decided `unsupported`: `<short name>/<row>`.
    Unsupported(&'static str),
    /// A set bit or a value no row accounts for (digest-kinds §2.1).
    Unaccounted(&'static str),
    /// digest §3.4.
    Collision,
    /// A local type no unit's walk declares (digest §4.3).
    UndeclaredLocal,
    /// An area of the walk that is not written yet. Removed from this
    /// enumeration, with `Area`, when the last area lands.
    Pending(Area),
}

impl Cause {
    /// The last word of the counter `digest.indeterminate.<cause>`.
    pub fn name(&self) -> &'static str {
        match self {
            Cause::Error => "error",
            Cause::Unsupported(_) => "unsupported",
            Cause::Unaccounted(_) => "unaccounted",
            Cause::Collision => "collision",
            Cause::UndeclaredLocal => "undeclared_local",
            Cause::Pending(_) => "pending",
        }
    }
}

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cause::Unsupported(what) | Cause::Unaccounted(what) => {
                write!(f, "{} {what}", self.name())
            }
            Cause::Pending(area) => write!(f, "pending {}", area.name()),
            Cause::Error | Cause::Collision | Cause::UndeclaredLocal => f.write_str(self.name()),
        }
    }
}

/// An area of the walk that a later work package writes (WP-024's table
/// "Internal"). Removed when the last one lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Area {
    /// The payload records and cold maps of declarations (digest-kinds
    /// §5.4 to §5.8): `decl_func.rs`, `decl_class.rs`, `decl_scope.rs`.
    DeclData,
    /// Declarations walked in place (§2.4): `local.rs`.
    Local,
    /// Statements (§6): `stmt.rs`.
    Stmt,
    /// Expressions and canonical expression operands (§7, §4.4):
    /// `expr.rs`.
    Expr,
    /// Constant values and strings (§8, §10.3): `value.rs`.
    Value,
    /// Templates beyond what keys need (§9): `templ.rs`.
    Templ,
    /// Attributes and annotations (§10.1, §10.5): `attrs.rs`.
    Attrs,
    /// The other tables of `Ast` (§10): `other.rs`.
    Other,
    /// Demand-driven fields and resolved units (§11): `demand.rs`.
    Demand,
    /// The order unit (digest §2.1).
    Order,
}

impl Area {
    /// Its name in counters and messages.
    pub fn name(self) -> &'static str {
        match self {
            Area::DeclData => "decl_data",
            Area::Local => "local",
            Area::Stmt => "stmt",
            Area::Expr => "expr",
            Area::Value => "value",
            Area::Templ => "templ",
            Area::Attrs => "attrs",
            Area::Other => "other",
            Area::Demand => "demand",
            Area::Order => "order",
        }
    }
}

/// Digests one translation unit.
///
/// Never fails: what cannot be digested is an indeterminate unit.
pub fn digest(input: &Input<'_>, options: &Options<'_>) -> Output {
    walk::run(input, options)
}
