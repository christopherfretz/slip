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
//! | `demand::deferrable(w, f)` | whether a function is of the deferrable class (§11.2) |
//! | `expr::child(w, s, e)`, `expr::child_opt`, `expr::requires_clause` (`expr.rs`) | `child`: one expression, its common rows, its kind's, `ColdExprData`'s and `templ::expr_maps` (§7); a kind not yet filled is pending (`Area::Expr`) |
//! | `expr::child_keep_paren(w, s, e)` | `child`, an outermost `ParenExpr` written with its rows: for a `DependentDecltype` profile (§7 "Parentheses") |
//! | `expr_dep::profile(w, s, p)`, `expr_dep::profile_expr(w, s, e)` | `profile`: a canonical expression operand (§4.4), walked with the stream's expression state put aside and a fresh numbering frame (`walk::ExprState::profile`), both restored after; the outermost parenthesis kept |
//! | `expr_dep::profile_local(w, s, d)` | `ref` of a local object while a profile is walked (`Walk::write_ref`): declared inside it, the frame's `Ref::local`; a function parameter, `Ref::param` (scope depth, position); else the unit's ordinal |
//! | `expr_dep::decl_name(w, raw)` | nothing: the `Name` (§3.2) of a raw `DeclName` word, as `Walk::plain_name` names a declaration, with its cause |
//! | `stmt::child(w, s, st)`, `stmt::child_opt` (`stmt.rs`) | `child`: one statement, its rows in table order (§6), one level of `walk::NODE_LIMIT` for a chain of tails (`stmt.rs` "The walk") |
//! | `value::value(w, s, v)`, `value::string(w, s, l)` (`value.rs`) | `value`: a constant in place by structure, a constant it points to by content digest (§8); `string`: a string literal by content (§10.3) |
//! | `local::write_decl(w, s, d)` | `decl` (§2.4): a local entity's ordinal and stream, a block-scope variable with linkage a `ref`, a block-scope function a `ref` and its own default arguments; a local type its number, then its stream and its members' the first time (§4.3) |
//! | `local::owner(w, d)` | nothing: the owner of a local entity (§4.3), `None` when no stream numbers it |
//! | `local::numbering(w, o)` | nothing: owner `o`'s local types and objects by declaration (§4.3 "Numbering an owner"), memoised; `local::number_all(w)` numbers every owner before the first unit and seals the memo |
//! | `local::type_number(w, t)`, `local::known_number(w, t)` | nothing: a local type's `n` (the second reads the memo only, for `names.rs`) |
//! | `local::unit_of(w, d)`, `local::outer(w, d)` | nothing: the unit whose stream holds a local declaration; the owner's key and index a `Ref::outer` writes (§4.1) |
//! | `local::stream_source(w, e)` | nothing: the declaration whose stream the unit of `e` writes, which the numbering walks |
//! | `local::lambda_expr(w, s, e)` | `LambdaExpr.a` (§7): the closure's `decl`, rule 6's causes inside a key or a type |
//! | `decl_func::{function_data, var_data, param_data, let_data}(w, s, d)` | one `record T` of §5.4 each; a function's and a variable's language linkage (§5.6 `ScopeData`) |
//! | `demand::effective_kind(w, e)` | nothing: the one specialisation kind the digest reads for an entity (§11.10), from its own `SpecInfo`, an explicitly specialised member's own kind, or the greater of a member's own kind and its class's |
//! | `demand::definition_class(w, e)` | nothing: whether the entity is of the §11.9 definition class, whose definition is the resolved unit `definition` |
//! | `demand::pattern_holds_placeholder(w, e)` | nothing: whether the declared type of the pattern an instantiation comes from holds a placeholder (§11.9: the type is then `definition`'s) |
//! | `demand::write_resolved(w, e, owner, p, disc, body)` | a resolved unit of `owner` (§11.1): its key, class, file and cause; `body` writes its stream |
//! | `decl_func::{function_definition, var_definition}(w, s, e, def)`, `decl_class::{record_definition, enum_definition}(w, s, def)`, `templ::spec_definition(w, s, e)` | the rows of the resolved unit `definition` (§11.9) |
//! | `decl_func::var_type(w, d, e)` | nothing: the type a variable's unit holds, an array of unknown bound completed by another declaration |
//! | `units::defining_decl(w, e)` | nothing: the defining declaration of a variable ([basic.def]) or Kiln's `definition` of another entity |
//! | `Walk::linkage_spec(d, all)` (`keys.rs`) | nothing: the innermost `LinkageSpec` around `d` and its language (`None` for an unknown byte), through namespaces only unless `all` |
//! | `decl_class::{typedef_data, enum_data, enumerator_data, record_data, field_data}(w, s, d)` | one `record T` of §5.5 each |
//! | `decl_class::ctor_init(w, s, i)` | `record CtorInit` (§5.5), for `ColdDeclData` `ctor_inits` |
//! | `decl_scope::{friend_target, namespace_data, static_assert_data, file_scope_asm, cold}(w, s, d)` | one `record T` of §5.6 each, and the rows of `ColdDeclData` (§5.7), each map read from the declaration Kiln keys it by |
//! | `templ::templ_link(w, s, l)` | `record TemplLink` (§9.1), which brings in `SpecInfo`, `MemberSpecInfo` or `PartialSpecInfo` (§9.4) |
//! | `templ::expr_maps(w, s, e)` | the `in` rows of `TemplateTables` keyed by `ExprId` (§9.5), each in its empty form when absent |
//! | `local::lambda_data(w, s, l)` | `record LambdaData` (§10.4) with its captures (`LambdaCapture`, `CaptureKind`, `LambdaMangling`) |
//! | `fp::fp_override(s, o)` | `record FpOverride` (§6.1) |
//! | `attrs::decl_attrs(w, s, d)` | `attrs` of a declaration over its chain, a list per declaration, the definition's apart (a parameter's: parameter i of its function's declarations), then `AnnotArena` `of_decl` and `of_base` (§10.1, §10.5) |
//! | `attrs::stmt_attrs(w, s, l)` | `attrs` of a statement (`AttributedStmt.a`): the nodes of list `l` in stored order (§10.1) |
//! | `enclosing::write(w, s, e)` | the walk's row `enclosing` (§1.7) at the end of the unit of `e` when it is at namespace scope: its linkage, whether an unnamed namespace encloses it, and the visibility each declaration takes from its namespaces (WP-113) |
//! | `other::module_rows(w, s, d)` | the rows `points` and `explicit_defs` of `ModuleTable` (§10.7), from the first declaration; empty forms when ownership is not tracked |
//! | `order::write(w, s)` | the order unit's stream (digest §2.1): the keys of the variable definitions at namespace and class scope in order, then the translation unit's imports (§10.7) |
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
//! Statements and expressions nest far deeper than 200 (LLVM has call and
//! operator chains thousands of levels deep), so they have their own
//! count: `Walk::enter_node`/`leave_node`, limit `walk::NODE_LIMIT`
//! (10 000), past which the stream is indeterminate with the same cause.
//! The expression walk does not recurse for the kinds `expr.rs` and
//! `expr_core.rs` write (`expr.rs` "The walk"): their children are items
//! of one heap stack, so an operator, call, member or cast chain of any
//! length takes one level (`tests/expr_chains.rs`: 20 000 operators and
//! 4 000 member calls, linear). A level is entered by each `expr::child`
//! (a kind function of `expr_init.rs`, `expr_dep.rs`, `expr_misc.rs`
//! calling it, a declaration or type reaching an expression) and is to be
//! entered by each statement the statements package walks.
//!
//! `digest` therefore needs a stack of [`STACK_SIZE`] (64 MiB): it must
//! run on a thread of at least that size (the tests' runner does,
//! `tests/common/mod.rs` `on_walk_stack`). The figure comes from measured
//! frames (`expr::tests::frames_fit_the_stack`): one node level is about
//! 820 bytes (the path a kind of another file takes: `child`, `walk`,
//! `walk_items`, the kind's function, `child` again), the walk at
//! `DEPTH_LIMIT` levels of types and keys about 0.74 MB, so both limits
//! need about 9.0 MB; `STACK_SIZE` is more than four times that, which
//! the test asserts. A package whose recursion through `enter_node` has
//! larger frames (a statement's walk) adds its path to that test.
//!
//! A lambda-expression is a `DEPTH_LIMIT` level (the closure's `decl`),
//! not a node level: `stmt::tests::frames_fit_the_stack` measures a
//! nesting of lambdas (about 7.2 KB each, 9.7 KB through a statement
//! expression) and asserts `NODE_LIMIT` statement levels plus
//! `DEPTH_LIMIT` lambda nestings fit in a quarter of `STACK_SIZE`
//! (`tests/local_chains.rs`: 150 nestings are `walk/depth`). Numbering an
//! owner (`local.rs`) walks the owner's stream in a mode of the same walk;
//! `local::number_all` numbers every owner before the first unit, so a
//! numbering never runs on top of a unit's walk and the stack holds one
//! walk at a time (a numbering asked for later is `local/late-numbering`,
//! which the pair tests check never happens).
//!
//! These are frames of the test profile only. The worker package must
//! measure the release profile, on the corpus, before `STACK_SIZE` (and
//! the scheme that runs `digest`) is fixed.
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
//! # How to fill an expression kind
//!
//! `expr::dispatch` lists all 97 `ExprKind` variants. A kind of
//! `expr_init.rs`, `expr_dep.rs` or `expr_misc.rs` is a function
//! `pub(crate) fn <snake_kind>(w, s, e)` of that file, whose body is
//! `s.pending(Area::Expr)` until its package fills it; `expr.rs` need not
//! change. `child` has already written the common rows (`kind`, the value
//! category, the object kind, `deps`, `ty`) and writes the `ColdExprData`
//! rows and `templ::expr_maps` after the function returns, so the function
//! writes the rows of its kind only, in table order. To read the node:
//! `expr::node(w, e)` (kind, the per-kind bits 4-7 in place, `aux`, `a`,
//! `b`, `ty`) and the `ExprArena` accessors (`w.ast.exprs`); to turn a
//! word into an id, `expr::expr_id`, `expr::decl_id`, `expr::expr_list`
//! (a word of 0 is the given `Unaccounted` cause, never a panic); to write
//! children, `expr::child`, `expr::child_opt`, `expr::children`
//! (`child*`); for bits and `aux` values no row accounts for,
//! `expr::check_bits` and `expr::check_aux`. The rows of `ColdExprData`
//! that a kind owns are functions of its file too
//! (`expr_init::designators`, `expr_misc::array_trait_extra`,
//! `expr_misc::offsetof_components`, `expr_misc::offsetof_layout`), called
//! by `expr::cold` where their row stands.
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
mod enclosing;
mod expr;
mod expr_core;
mod expr_dep;
mod expr_init;
mod expr_misc;
mod expr_profile;
mod fp;
mod groups;
mod keys;
mod local;
pub mod map;
mod names;
mod order;
mod other;
mod owned;
pub mod query;
pub mod records;
mod refs;
mod rule_use;
pub mod spelling;
mod stmt;
mod stream;
mod templ;
mod types;
mod units;
mod value;
mod walk;

#[cfg(test)]
mod tests;

pub use names::display_name;
pub use owned::{ReadOnly, ReadOnlyFiles};
pub use walk::STACK_SIZE;

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
    /// Which files of the image are read-only (`spec/verify.md` §2.1):
    /// under a non-empty `rules`, a rule matches an entity none of whose
    /// declarations lies in one, and the namespaces around a member are
    /// mapped from the member's point of view (`owned.rs`). Not read when
    /// `rules` is empty. [`ReadOnly::EVERY_FILE`] where the run's files
    /// are not known; [`ReadOnly::files`] of [`ReadOnlyFiles`] for the verify
    /// job.
    pub read_only: ReadOnly<'a>,
    pub detail: Detail<'a>,
    /// Fill `Output::names`.
    pub names: bool,
    /// Fill `Output::rule_use` (`spec/verify.md` §9.1, §9.5).
    pub rule_use: bool,
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
    /// How each rule was used (`spec/verify.md` §9.1): sorted by rule,
    /// only rules with `via` or `direct` not 0; empty unless asked for
    /// under a table that is not empty (`rule_use.rs`).
    pub rule_use: Vec<slip_model::agreement::Use>,
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
    /// An entity at namespace scope whose declarations take different
    /// visibilities from their namespace openings: which one a use's
    /// symbol takes depends on where the use is (digest §10, WP-113).
    VisibilityByUse,
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
            Cause::VisibilityByUse => "visibility_by_use",
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
            Cause::Error | Cause::Collision | Cause::UndeclaredLocal | Cause::VisibilityByUse => {
                f.write_str(self.name())
            }
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
    /// The order unit (digest §2.1).
    Order,
    /// A fact a canonical profile leaves out but its first spelling
    /// holds: the declarations lookup found for an unresolved name in a
    /// dependent type (digest-kinds §4.4, WP-036). Written by the unit
    /// stream when the spelling is (WP-041).
    Spelling,
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
            Area::Order => "order",
            Area::Spelling => "spelling",
        }
    }
}

/// Digests one translation unit.
///
/// Never fails: what cannot be digested is an indeterminate unit.
pub fn digest(input: &Input<'_>, options: &Options<'_>) -> Output {
    walk::run(input, options)
}
