//! Constant values and strings (`spec/digest.md` §7,
//! `spec/digest-kinds.md` §8, §8.1, §10.3).
//!
//! A constant is read only through `ConstStore::get` (the decoded
//! `ConstValue`) and written by structure: the tag of its `ValueKind`,
//! then the `in` rows of its `Scalar` variant or of its `ByteImage`.
//! Nothing written depends on a `ConstValueId` or on the order Kiln
//! interned the values in; the stored words of §8.1 (`Node`, `AggRec`,
//! `Pools`, `ConstStore`) are all `out`.
//!
//! The value `value` is given, and each relocation of an image (a pointer
//! scalar), are written in place. A pointer to another constant of the
//! store (`PtrBase::Const`) is written by that constant's content (digest
//! §7): the 32-byte digest `H("slip.value.v1", s)` of the stream `s` that
//! writing it in place would give, its cause handed to the stream that
//! uses it ([`const_digest`]). A constant shared by many pointers is so
//! written once per walk, not once per path, and the work is linear in
//! the constants reached (`spec/landed/digest-kinds/WP-028.md`).
//!
//! What bounds the work and the stack: the store is append-only and a
//! value can only name values interned before it (`ConstStore::push_scalar`
//! rejects a `PtrBase::Const` id not yet in the store, and `push_aggregate`
//! a relocation to a value that is not pointer-like), so the constants a
//! value reaches form a finite DAG whose edges go to smaller ids. Their
//! content digests are computed in increasing id order, each child before
//! its parents, so computing one never recurses into another: the
//! nesting is one level (lib.rs "What bounds the stack") however long the
//! chain. One constant's stream (its image and its relocations in place)
//! does at most [`VALUE_BUDGET`] units of work.
//!
//! A content digest is computed as a key is (`spec/landed/digest/WP-024.md`
//! "A key does not depend on the unit that asks"): the unit's local state
//! is put aside, so a local entity met below is
//! `Unaccounted("key/local-ref")`, and the digest is memoised in the walk
//! by `ConstValueId` unless a depth hit or a cycle was met below it.
//!
//! "Padding zeroed" (digest §7): Kiln's image already has it.
//! `ConstStore::push_aggregate` (`kiln_ast/src/consts/mod.rs`, through
//! `Pools::check_image`, lines 632 to 692 at the pinned Kiln) rejects an
//! image with a non-zero byte under a padding range ("non-zero padding
//! byte") or under a relocation ("non-zero bytes under a relocation"), and
//! a store restored from a snapshot is checked the same way
//! (`consts/snap.rs:102`), so `ByteImage::bytes` is
//! written as stored and the ranges that hold no value are written beside
//! it by the rows (`padding`, `relocs`); the shadow runs
//! (`ConstStore::shadow`) are computed from those and are not read.

use std::collections::{HashMap, HashSet};

use slip_kiln::kiln::ast::consts::{
    ByteImage, ByteRange, ConstValue, FloatKind, MemberKind, NullKind, PtrBase, Reloc, Scalar,
    UnionMember,
};
use slip_kiln::kiln::ast::decl::LetData;
use slip_kiln::kiln::ast::expr::{ExprKind, PredefinedKind};
use slip_kiln::kiln::ast::{ConstValueId, DeclId, ExprId, StringLitId};
use slip_model::Digest;

use crate::Cause;
use crate::refs::REF_NONE;
use crate::stream::{Hashed, Stream};
use crate::tables::{
    Decision, Row, float_kind, member_kind, null_kind, ptr_base, string_encoding, value_kind,
};
use crate::walk::{TOO_DEEP, Walk};

/// The work one constant's stream may do: one unit per value written in
/// place and one per image byte. Past it the stream is indeterminate with
/// [`TOO_LARGE`]. 2^26 is far above any constant a program holds (a
/// 64 MiB image).
const VALUE_BUDGET: u64 = 1 << 26;

/// The cause of a value larger than [`VALUE_BUDGET`].
const TOO_LARGE: Cause = Cause::Unaccounted("value/size");

/// A constant whose content digest was not computed before a value that
/// names it: the store's order (a value names only earlier values) makes
/// this unreachable; it is a shape the walk does not account for.
const OUT_OF_ORDER: Cause = Cause::Unaccounted("value/order");

/// `H("slip.value.v1", stream)`: the content digest of a constant a
/// pointer names (`spec/landed/digest-kinds/WP-028.md`).
pub(crate) const VALUE_TAG: &str = "slip.value.v1";

/// What a value that cannot be written puts in its place: `uv 0`, which
/// no tag is (digest-kinds §1.4), as the other areas do.
const PLACEHOLDER: u64 = 0;

/// The state of writing one value.
struct Ctx {
    /// What is left of [`VALUE_BUDGET`] for the stream being written.
    budget: u64,
    /// Content digests computed in this call that are not memoised in the
    /// walk (a depth hit or a cycle below them): their cause depends on
    /// the path, so they serve this call only.
    local: HashMap<u32, Hashed>,
    /// A digest of `local` was used by the stream being written.
    tainted: bool,
    /// Content digests are being computed ([`compute`]).
    computing: bool,
}

impl Ctx {
    fn new() -> Ctx {
        Ctx { budget: VALUE_BUDGET, local: HashMap::new(), tainted: false, computing: false }
    }
}

/// `value`: the constant `v` by the rows of §8.
pub(crate) fn value(w: &mut Walk<'_>, s: &mut Stream, v: ConstValueId) {
    // Numbering an owner reads no value (`local.rs`).
    if crate::local::numbering_mode(w) {
        return;
    }
    let mut cx = Ctx::new();
    write_value(w, s, v, &mut cx);
}

/// `string`: a string literal by content, the rows of its `Entry`
/// (§10.3): `start` as the code-unit bytes Kiln stores, then `enc`.
/// `StringPool::get` hands the entry over as a tuple, not as the record,
/// so a field Kiln adds to `Entry` does not fail to compile here.
pub(crate) fn string(w: &mut Walk<'_>, s: &mut Stream, l: StringLitId) {
    let (enc, bytes) = w.ast.strings.get(l);
    s.bytes(bytes).tag(string_encoding::row(&enc));
}

/// Takes `n` units of the budget; `false` (and the stream is
/// indeterminate) once it is spent.
fn spend(s: &mut Stream, cx: &mut Ctx, n: u64) -> bool {
    if cx.budget < n {
        cx.budget = 0;
        s.indeterminate(TOO_LARGE);
        return false;
    }
    cx.budget -= n;
    true
}

/// The tag of a variant row, and the row's cause if it is not `in`
/// (digest-kinds §1.2); `name` is `<short name>/<row>`.
fn variant(s: &mut Stream, row: Row, name: &'static str) {
    s.tag(row);
    match row.decision {
        Decision::In => {}
        Decision::Unsupported => {
            s.indeterminate(Cause::Unsupported(name));
        }
        Decision::Out | Decision::Symbolic | Decision::Demand => {
            s.indeterminate(Cause::Unaccounted(name));
        }
    }
}

/// One value in place: `enum ValueKind`, then `record Scalar` or `record
/// ByteImage` (`ConstValue` `Scalar.0`, `Aggregate.0`).
fn write_value(w: &mut Walk<'_>, s: &mut Stream, v: ConstValueId, cx: &mut Ctx) {
    if !spend(s, cx, 1) {
        s.uv(PLACEHOLDER);
        return;
    }
    if !w.enter() {
        s.uv(PLACEHOLDER).indeterminate(TOO_DEEP);
        return;
    }
    let ast = w.ast;
    match ast.consts.get(v) {
        ConstValue::Scalar(x) => write_scalar(w, s, x, cx),
        ConstValue::Aggregate(img) => write_image(w, s, img, cx),
    }
    w.leave();
}

/// The rows of the `Scalar` variant, after its `ValueKind` tag.
fn write_scalar(w: &mut Walk<'_>, s: &mut Stream, x: Scalar<'_>, cx: &mut Ctx) {
    match x {
        Scalar::Int { ty, signed, bits } => {
            variant(s, value_kind::INT, "ValueKind/Int");
            w.write_type(s, ty);
            s.bool(signed);
            // `Int.bits`: the normalised words, each little-endian.
            let words = bits.words();
            let mut b = Vec::with_capacity(words.len() * 8);
            for word in words {
                b.extend_from_slice(&word.to_le_bytes());
            }
            s.bytes(&b);
        }
        Scalar::Bool { ty, value } => {
            variant(s, value_kind::BOOL, "ValueKind/Bool");
            w.write_type(s, ty);
            s.bool(value);
        }
        Scalar::Float { ty, kind, bits } => {
            variant(s, value_kind::FLOAT, "ValueKind/Float");
            w.write_type(s, ty);
            variant(s, float_kind::row(&kind), float_name(kind));
            // `Float.bits`: the exact encoding, 16 bytes little-endian.
            s.bytes(&bits.to_le_bytes());
        }
        Scalar::Null { ty, kind } => {
            variant(s, value_kind::NULL, "ValueKind/Null");
            w.write_type(s, ty);
            variant(s, null_kind::row(&kind), null_name(kind));
        }
        Scalar::Pointer { ty, base, offset, designated, past } => {
            variant(s, value_kind::POINTER, "ValueKind/Pointer");
            w.write_type(s, ty);
            let named = write_base(w, s, base, cx);
            s.uv(offset);
            // `void *` into a name-derived string designates its array:
            // the bound is the name's, as the base's bytes are (digest §9).
            match designated
                .and_then(|d| if named { w.unbounded_array(d).map(|t| (t, d)) } else { None })
            {
                Some((array, written)) => {
                    s.opt(true);
                    w.absorb_type(s, array, written, None);
                }
                None => w.write_type_opt(s, designated),
            }
            s.bool(past);
        }
        Scalar::MemberPtr { ty, member, kind, derived, path } => {
            variant(s, value_kind::MEMBER_PTR, "ValueKind/MemberPtr");
            w.write_type(s, ty);
            w.write_ref(s, member);
            variant(s, member_kind::row(&kind), member_name(kind));
            s.bool(derived);
            w.write_refs(s, path);
        }
        Scalar::Reflection { ty: _, id: _ } => {
            // `ValueKind/Reflection`, `Scalar/Reflection.ty` and `.id`
            // are `unsupported` (§12): the tag alone keeps the stream
            // well formed.
            variant(s, value_kind::REFLECTION, "ValueKind/Reflection");
        }
    }
}

/// The cause name of a `FloatKind` row.
fn float_name(k: FloatKind) -> &'static str {
    match k {
        FloatKind::Half => "FloatKind/Half",
        FloatKind::BFloat16 => "FloatKind/BFloat16",
        FloatKind::Single => "FloatKind/Single",
        FloatKind::Double => "FloatKind/Double",
        FloatKind::X87Extended => "FloatKind/X87Extended",
        FloatKind::Quad => "FloatKind/Quad",
    }
}

/// The cause name of a `NullKind` row.
fn null_name(k: NullKind) -> &'static str {
    match k {
        NullKind::Pointer => "NullKind/Pointer",
        NullKind::MemberData => "NullKind/MemberData",
        NullKind::MemberFunction => "NullKind/MemberFunction",
    }
}

/// The cause name of a `MemberKind` row.
fn member_name(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Data => "MemberKind/Data",
        MemberKind::Function => "MemberKind/Function",
    }
}

/// The cause name of a `PtrBase` row.
fn base_name(b: &PtrBase) -> &'static str {
    match b {
        PtrBase::Decl(_) => "PtrBase/Decl",
        PtrBase::Function(_) => "PtrBase/Function",
        PtrBase::Temporary(_) => "PtrBase/Temporary",
        PtrBase::StringLit { lit: _, origin: _ } => "PtrBase/StringLit",
        PtrBase::Const(_) => "PtrBase/Const",
        PtrBase::TypeInfo(_) => "PtrBase/TypeInfo",
        PtrBase::Label { label: _, origin: _ } => "PtrBase/Label",
        PtrBase::Absolute => "PtrBase/Absolute",
        PtrBase::LabelDiff { plus: _, minus: _, size: _ } => "PtrBase/LabelDiff",
    }
}

/// `enum PtrBase`; `true` when the base is a name-derived object written
/// symbolically (digest §9, [`NameValues`]).
fn write_base(w: &mut Walk<'_>, s: &mut Stream, base: PtrBase, cx: &mut Ctx) -> bool {
    variant(s, ptr_base::row(&base), base_name(&base));
    match base {
        PtrBase::Decl(d) | PtrBase::Function(d) => w.write_ref(s, d),
        PtrBase::Temporary(d) => write_temporary(w, s, d),
        PtrBase::StringLit { lit, origin } if w.by_value == 0 => {
            match name_string(w, lit, origin) {
                Some(which) => {
                    write_name_string(w, s, which);
                    return true;
                }
                None => string(w, s, lit),
            }
        }
        PtrBase::StringLit { lit, origin: _ } => string(w, s, lit),
        PtrBase::Const(c) if w.by_value == 0 && name_values(w).objects.contains_key(&c.index()) => {
            write_location(w, s, c);
            return true;
        }
        PtrBase::Const(c) => {
            let h = const_digest(w, cx, c);
            s.digest(&h.digest).absorb(h.cause);
        }
        PtrBase::TypeInfo(q) => w.write_type(s, q),
        PtrBase::Label { label, origin: _ } => w.write_ref(s, label),
        PtrBase::Absolute => {}
        PtrBase::LabelDiff { plus, minus, size } => {
            write_addr_label(w, s, plus, "PtrBase/LabelDiff.plus");
            write_addr_label(w, s, minus, "PtrBase/LabelDiff.minus");
            s.uv(u64::from(size));
        }
    }
    false
}

// ---------------------------------------------------------------------
// Values that come from names and positions (digest §9, WP-104).
//
// A symbolic value has the identity of the object it stands for: two are
// written alike only if they are the same string (or `__impl` object) of
// the same entity. A string is written as the entity whose name it holds
// only where its bytes are what Kiln makes for that entity (checked here
// by printing the name again with Kiln's own printer), so two strings
// written alike hold the names of one entity (mapped by the rules), and
// differ by nothing but what §9 leaves out of the digest. Anywhere else
// the bytes are written.

/// The objects of the constant store that hold the values of the
/// source-location builtins (`kiln_sema/src/builtins.rs` `record_source_loc`,
/// `srcloc_string`, `source_location_object`): Kiln makes one string
/// object per content, whose `StringLiteral` has no parent
/// (`frozen_string`), and one `std::source_location::__impl` object per
/// use. They are found from the values Kiln records for every use
/// (`ExprArena` `cold.source_loc`), once per walk; so are the function
/// bodies a `PredefinedExpr`'s string is looked up in.
#[derive(Debug, Default)]
pub(crate) struct NameValues {
    /// The `ExprId`s (index) of the source-location strings' literals:
    /// each use that points to one, as `(role, context, use)`.
    origins: HashMap<u32, Vec<(Role, DeclId, ExprId)>>,
    /// The `ConstValueId`s (index) of the `__impl` objects, each claimed
    /// by its first use (`ExprId` order; Kiln shares one object between
    /// uses of equal values).
    objects: HashMap<u32, Impl>,
    /// Every function definition's extent, as file locations (raw),
    /// sorted: the first written mem-initializer, or the `try` of a
    /// function-try-block, or the body's `{`, to the body's `}` or the
    /// last handler's.
    bodies: Vec<(u32, u32, DeclId)>,
    /// The definitions whose extent's ends are in two inclusions (a body
    /// that ends in an included file): where one of them has a name
    /// string's name, the string's bytes are written.
    split: Vec<DeclId>,
    /// The entity found for a `PredefinedExpr`'s string, by literal.
    owners: HashMap<u32, Option<DeclId>>,
    /// What a source-location string object is (by its literal's
    /// `ExprId` index).
    sourced: HashMap<u32, Option<NameString>>,
}

/// What identifies an `__impl` object (digest §9, WP-115 item 2).
#[derive(Clone, Copy, Debug)]
struct Impl {
    /// Where its first use is numbered.
    group: Group,
    /// That use's ordinal among the uses of its group that build an
    /// `__impl` object, in `ExprId` order (the holder's own order: Kiln
    /// numbers a holder's expressions as it builds them). Every use is
    /// counted, also one whose object an earlier use built (Kiln shares
    /// one object between uses of one file, function, line and column),
    /// so an object's ordinal does not depend on the layout the digest
    /// drops (WP-115 fix round, F1).
    n: u64,
    /// The literal of the file name it points to (always one: below,
    /// [`impl_file`]).
    file: Option<StringLitId>,
}

/// The group an `__impl` object's use is numbered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Group {
    /// The use's context is this function.
    Function(DeclId),
    /// The use's context is no function (a namespace or a class: the
    /// initializer of a variable or of a field, a default argument):
    /// numbered among the uses of the declaration whose initializer or
    /// default argument holds it ([`holders`]), by its entity; `None`
    /// where no such declaration holds it (an enumerator, a template
    /// argument, a bound), numbered among every such use of the
    /// translation unit.
    Holder(Option<DeclId>),
}

/// What a source-location string is to a use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// `file` or `file_name`, or the file of an `__impl` object.
    File,
    /// `function` (`__builtin_FUNCTION()`): the context's plain name.
    Plain,
    /// The function name of an `__impl` object: the context's pretty name.
    Pretty,
}

/// Which name-derived string a `PtrBase::StringLit` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameString {
    /// The value of a `PredefinedExpr` of this kind in this function.
    Predefined(PredefinedKind, DeclId),
    /// A source-location function name (plain or pretty) of this context.
    SourceLoc(Role, DeclId),
}

/// The [`NameValues`] of the walk, built on first use.
fn name_values<'w>(w: &'w mut Walk<'_>) -> &'w mut NameValues {
    let ast = w.ast;
    let sm = w.sm;
    w.name_values.get_or_insert_with(|| {
        let mut nv = NameValues::default();
        let origin_of = |c: ConstValueId| match ast.consts.scalar(c) {
            Some(Scalar::Pointer {
                ty: _,
                base: PtrBase::StringLit { lit: _, origin },
                offset: _,
                designated: _,
                past: _,
            }) => Some(origin),
            _ => None,
        };
        let lit_of = |c: ConstValueId| match ast.consts.scalar(c) {
            Some(Scalar::Pointer {
                ty: _,
                base: PtrBase::StringLit { lit, origin: _ },
                offset: _,
                designated: _,
                past: _,
            }) => Some(lit),
            _ => None,
        };
        let mut uses: Vec<(ExprId, slip_kiln::kiln::ast::expr::SourceLocValues)> =
            ast.exprs.cold.source_loc.iter().map(|(e, v)| (e, *v)).collect();
        uses.sort_unstable_by_key(|&(e, _)| e.index());
        let held = holders(ast, &uses);
        let mut ordinals: HashMap<Group, u64> = HashMap::new();
        for (e, v) in uses {
            let slip_kiln::kiln::ast::expr::SourceLocValues {
                file,
                file_name,
                function,
                line: _,
                column: _,
                location,
                at: _,
                ctx,
            } = v;
            let add = |nv: &mut NameValues, c: ConstValueId, role: Role| {
                if let Some(o) = origin_of(c) {
                    nv.origins.entry(o.index()).or_default().push((role, ctx, e));
                }
            };
            add(&mut nv, file, Role::File);
            add(&mut nv, file_name, Role::File);
            add(&mut nv, function, Role::Plain);
            let Some(l) = location else { continue };
            let Some(Scalar::Pointer {
                ty: _,
                base: PtrBase::Const(obj),
                offset: _,
                designated: _,
                past: _,
            }) = ast.consts.scalar(l)
            else {
                continue;
            };
            if let ConstValue::Aggregate(ByteImage {
                ty: _,
                bytes: _,
                relocs,
                padding: _,
                unions: _,
            }) = ast.consts.get(obj)
            {
                let group = if ast.decls.get(ctx).kind().is_function() {
                    Group::Function(ctx)
                } else {
                    Group::Holder(held.get(&e.index()).copied())
                };
                let n = ordinals.entry(group).or_insert(0);
                let this = *n;
                *n += 1;
                nv.objects.entry(obj.index()).or_insert(Impl {
                    group,
                    n: this,
                    file: lit_of(file),
                });
                let file_origin = origin_of(file);
                for &Reloc { at: _, value } in relocs {
                    let role =
                        if origin_of(value) == file_origin { Role::File } else { Role::Pretty };
                    add(&mut nv, value, role);
                }
            }
        }
        (nv.bodies, nv.split) = definition_extents(ast, sm);
        nv
    })
}

/// The declaration that holds each use of `uses` that builds an `__impl`
/// object outside a function (by the use's `ExprId` index): the
/// variable or field whose initializer, or the function whose
/// parameter's default argument, holds the use's expression, as its
/// entity's first declaration. Found by walking those expressions in
/// declaration order; the first declaration met holds a use met twice (a
/// shared expression). A use no such expression holds has none.
fn holders(
    ast: &slip_kiln::kiln::ast::Ast,
    uses: &[(ExprId, slip_kiln::kiln::ast::expr::SourceLocValues)],
) -> HashMap<u32, DeclId> {
    let wanted: HashSet<u32> = uses
        .iter()
        .filter(|(_, v)| v.location.is_some() && !ast.decls.get(v.ctx).kind().is_function())
        .map(|(e, _)| e.index())
        .collect();
    let mut out = HashMap::new();
    if wanted.is_empty() {
        return out;
    }
    let mut stack = Vec::new();
    for d in ast.decls.ids() {
        let (root, holder) = if let Some(v) = ast.decls.var(d) {
            (v.init, d)
        } else if let Some(f) = ast.decls.field(d) {
            (f.init, d)
        } else if let Some(p) = ast.decls.param(d) {
            match ast.decls.get(d).ctx() {
                Some(f) if ast.decls.get(f).kind().is_function() => (p.default_arg, f),
                Some(_) | None => (None, d),
            }
        } else {
            continue;
        };
        let Some(root) = root else { continue };
        let holder = ast.first(holder);
        stack.clear();
        stack.push(root);
        while let Some(x) = stack.pop() {
            if wanted.contains(&x.index()) {
                out.entry(x.index()).or_insert(holder);
            }
            ast.exprs.children_into(x, &ast.lists, &mut stack);
        }
        if out.len() == wanted.len() {
            break;
        }
    }
    out
}

/// Every function definition's extent ([`NameValues`] `bodies`), sorted:
/// the first written mem-initializer, or the `try` of a function-try-block,
/// or the body's `{`, to the body's `}` or the last handler's, as file
/// locations (raw).
#[allow(clippy::type_complexity, reason = "two lists, built together")]
fn definition_extents(
    ast: &slip_kiln::kiln::ast::Ast,
    sm: &slip_kiln::kiln::source::SourceManager,
) -> (Vec<(u32, u32, DeclId)>, Vec<DeclId>) {
    use slip_kiln::kiln::ast::stmt::StmtKind;
    let mut out = Vec::new();
    let mut split = Vec::new();
    for d in ast.decls.ids() {
        let Some(f) = ast.decls.function(d) else { continue };
        let Some(b) = f.body else { continue };
        let st = ast.stmts.get(b);
        let (lo, hi) = match st.kind() {
            StmtKind::CompoundStmt => (st.loc(), ast.stmts.rbrace(b)),
            // Kiln's `parse_function_definition`: `try` is the
            // statement's location, the last handler's `}` ends the
            // definition.
            StmtKind::CXXTryStmt => {
                let last = ast
                    .lists
                    .iter::<slip_kiln::kiln::ast::StmtId>(ast.stmts.try_handlers(b))
                    .last();
                let Some(h) = last else { continue };
                (st.loc(), ast.stmts.rbrace(ast.stmts.catch_block(h)))
            }
            _ => continue,
        };
        let mut lo = sm.expansion_loc(lo);
        let hi = sm.expansion_loc(hi);
        // A constructor's written mem-initializers come before its body
        // (or after `try`): each begins at its mem-initializer-id.
        for init in ast.ctor_inits(d).filter(|i| i.is_written()) {
            let at = sm.expansion_loc(init.loc);
            if at.is_file() && at.to_raw() < lo.to_raw() {
                lo = at;
            }
        }
        if !lo.is_file() || !hi.is_file() {
            continue;
        }
        // One inclusion of one file holds both ends: raw locations of a
        // range are ordered as its text, and a nested inclusion's range
        // lies outside it. Else the extent crosses an inclusion.
        let one = sm.offset_of(lo).map(|(f, _)| f) == sm.offset_of(hi).map(|(f, _)| f)
            && sm.include_loc(lo) == sm.include_loc(hi);
        if one && lo <= hi {
            out.push((lo.to_raw(), hi.to_raw(), d));
        } else {
            split.push(d);
        }
    }
    out.sort_unstable_by_key(|&(lo, hi, d)| (lo, hi, d.index()));
    split.sort_unstable_by_key(|d| d.index());
    (out, split)
}

/// The file location `at` and the `#include` locations of the inclusions
/// around it, innermost first, as raw locations (at most [`INCLUDE_DEPTH`]).
fn inclusion_chain(
    sm: &slip_kiln::kiln::source::SourceManager,
    at: slip_kiln::kiln::source::Loc,
) -> Vec<u32> {
    let mut out = Vec::new();
    let mut x = at;
    while x.is_file() && out.len() < INCLUDE_DEPTH {
        out.push(x.to_raw());
        x = sm.expansion_loc(sm.include_loc(x));
    }
    out
}

/// How many inclusions around a name string are looked through (Kiln's
/// own limit on nested inclusions is lower).
const INCLUDE_DEPTH: usize = 512;

/// Kiln's names of function `f` for a `PredefinedExpr` or a
/// source-location builtin (`kiln_sema/src/predefined.rs`
/// `compute_predefined_name`: the plain name, or `pretty_function`), with
/// either spelling of a closure; whether `bytes` is one of them.
fn is_kiln_name(w: &Walk<'_>, f: DeclId, pretty: bool, bytes: &[u8]) -> bool {
    if !w.ast.decls.get(f).kind().is_function() {
        return false;
    }
    [false, true].into_iter().any(|lambda| {
        let mut names = slip_kiln::kiln::ast::dump::AstNames::with_sources(w.ast, w.sm);
        names.closure_as_lambda = lambda;
        names.scoped_spec_args = true;
        let text = if pretty {
            slip_kiln::kiln::ast::dump::pretty_function(&names, f, true)
        } else {
            let mut t = String::new();
            names.write_name(f, &mut t);
            t
        };
        text.as_bytes() == bytes
    })
}

/// Whether string object `origin` (holding `lit`) holds a name-derived
/// value, and whose: the literal of a `PredefinedExpr` (Kiln builds it
/// immediately before its `PredefinedExpr`, `kiln_sema/src/predefined.rs:36-41`,
/// and reads it so, `kiln_sema/src/templ/args.rs:2022`; the
/// `PredefinedExpr`'s `a` names it, which is checked), in the innermost
/// function whose body holds the expression and whose name the bytes
/// are; or a source-location function name (Kiln makes one object per
/// content, shared by every use that has these bytes): of the one entity
/// every use of the object in the translation unit is in, whose name the
/// bytes are (WP-115 item 1). `None` (the bytes are written) for a file
/// name, where the uses are in several entities, and wherever no such
/// entity is found.
fn name_string(w: &mut Walk<'_>, lit: StringLitId, origin: ExprId) -> Option<NameString> {
    use slip_kiln::kiln::ast::RawId;
    let ast = w.ast;
    let (_, bytes) = ast.strings.get(lit);
    let next = origin.raw().checked_add(1).and_then(ExprId::from_raw);
    if let Some(next) = next
        && (next.raw() as usize) <= ast.exprs.len()
        && ast.exprs.get(next).kind() == ExprKind::PredefinedExpr
        && ast.exprs.predefined_name(next) == Some(origin)
    {
        let kind = ast.exprs.predefined_kind(next);
        let pretty = kind == PredefinedKind::PrettyFunction;
        if let Some(&owner) = name_values(w).owners.get(&origin.index()) {
            return owner.map(|f| NameString::Predefined(kind, f));
        }
        let at = w.sm.expansion_loc(ast.exprs.get(next).loc());
        let mut best: Option<(u32, DeclId)> = None;
        let mut tie = false;
        // The expression's location, then the `#include` of each
        // inclusion around it (a body may come from an included file,
        // WP-115 item 5, C2): the functions whose extent holds the first
        // of these that any extent holds are the innermost; among them,
        // the shortest extent (extents of one inclusion nest).
        let bodies = std::mem::take(&mut name_values(w).bodies);
        for at in inclusion_chain(w.sm, at) {
            for &(lo, hi, f) in bodies.iter().take_while(|&&(lo, _, _)| lo <= at) {
                if at > hi || !is_kiln_name(w, f, pretty, bytes) {
                    continue;
                }
                let span = hi - lo;
                match best {
                    Some((b, _)) if b < span => {}
                    Some((b, g)) if b == span && ast.first(g) != ast.first(f) => tie = true,
                    Some((b, _)) if b == span => {}
                    _ => {
                        best = Some((span, f));
                        tie = false;
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        name_values(w).bodies = bodies;
        let split = std::mem::take(&mut name_values(w).split);
        if split.iter().any(|&f| is_kiln_name(w, f, pretty, bytes)) {
            tie = true;
        }
        name_values(w).split = split;
        let owner = if tie { None } else { best.map(|(_, f)| f) };
        name_values(w).owners.insert(origin.index(), owner);
        return owner.map(|f| NameString::Predefined(kind, f));
    }
    if let Some(&found) = name_values(w).sourced.get(&origin.index()) {
        return found;
    }
    let found = sourced(w, bytes, origin);
    name_values(w).sourced.insert(origin.index(), found);
    found
}

/// [`name_string`] for a source-location string object `origin` holding
/// `bytes`: the context of its uses where every use of the object in the
/// translation unit is of one role and one entity (the function itself:
/// a use in a lambda or a local class of it is another entity's) and the
/// bytes are that entity's name of the role; else `None`, the bytes. A
/// pointer to the object can only have been made by one of these uses
/// (Kiln's `srcloc_string`; but see the landed file's named hole for
/// reflection strings), so where they are all of one entity the string is
/// certainly that entity's name.
fn sourced(w: &mut Walk<'_>, bytes: &[u8], origin: ExprId) -> Option<NameString> {
    let nv = name_values(w);
    let uses = nv.origins.get(&origin.index())?;
    let &(role, ctx, _) = uses.iter().min_by_key(|&&(_, _, e)| e.index())?;
    if role == Role::File {
        return None;
    }
    let uses = uses.clone();
    let one = w.entity(ctx);
    if uses.iter().any(|&(r, c, _)| r != role || w.entity(c) != one) {
        return None;
    }
    is_kiln_name(w, ctx, role == Role::Pretty, bytes).then_some(NameString::SourceLoc(role, ctx))
}

/// `PtrBase` `StringLit.lit` of a name-derived string (digest-kinds §8,
/// the rule under `PtrBase`): `bytes` empty, `uv 0` where a literal's
/// encoding tag stands (0 is no tag, so no literal is written so), then
/// `uv 1`, the `PredefinedKind` tag and the function, for a
/// `PredefinedExpr`'s value; `uv 2`, `uv 0` (plain) or `uv 1` (pretty)
/// and the context, for a source-location function name; then the names
/// of the named local types around that function
/// ([`crate::local::write_names_around`]), and for a pretty name those
/// its printed signature mentions ([`crate::local::write_signature_names`],
/// WP-115 item 5, H1).
fn write_name_string(w: &mut Walk<'_>, s: &mut Stream, which: NameString) {
    s.bytes(&[]).uv(0);
    match which {
        NameString::Predefined(k, f) => {
            s.uv(1).tag(crate::tables::predefined_kind::row(&k));
            w.write_ref(s, f);
            crate::local::write_names_around(w, s, f);
            match k {
                PredefinedKind::PrettyFunction => crate::local::write_signature_names(w, s, f),
                PredefinedKind::Func | PredefinedKind::Function => {}
            }
        }
        NameString::SourceLoc(role, ctx) => {
            s.uv(2).uv(u64::from(role == Role::Pretty));
            w.write_ref(s, ctx);
            crate::local::write_names_around(w, s, ctx);
            match role {
                Role::Pretty => crate::local::write_signature_names(w, s, ctx),
                Role::Plain | Role::File => {}
            }
        }
    }
}

/// `PtrBase` `Const.0` of a source-location `__impl` object: the digest
/// `H("slip.value.v1", uv 0, uv 3, type)` of its type (a value's own
/// stream begins with a `ValueKind` tag, never 0, so no constant written
/// by content has this digest), then where its use is numbered and its
/// ordinal there, then the file name it points to: `opt`, and the
/// literal as `string` writes a `__FILE__` literal's (§9: the file is not
/// dropped, the line and column are).
///
/// Where it is numbered: a use in a function is the function's (`ref`),
/// the names of the named local types around it and those its printed
/// signature mentions (the object holds its pretty name), and the use's
/// ordinal among the function's; a use outside every function is
/// `Ref::none`, `count 0`, then the declaration that holds it (`ref?`,
/// [`holders`]) with the names of the named local types around it, and
/// the use's ordinal among that declaration's (among every use no
/// declaration holds, where none does): numbered per holder, so a use
/// added to another holder does not renumber it (WP-115 item 2).
fn write_location(w: &mut Walk<'_>, s: &mut Stream, obj: ConstValueId) {
    let ty = match w.ast.consts.get(obj) {
        ConstValue::Aggregate(img) => Some(img.ty),
        ConstValue::Scalar(_) => None,
    };
    let mut inner = Stream::new();
    inner.uv(0).uv(3);
    match ty {
        Some(ty) => w.write_type(&mut inner, ty),
        None => {
            inner.indeterminate(Cause::Unaccounted("value/source-location"));
        }
    }
    let hashed = w.hashed(inner.finish(VALUE_TAG));
    s.digest(&hashed.digest).absorb(hashed.cause);
    let Impl { group, n, file } = name_values(w).objects[&obj.index()];
    match group {
        Group::Function(f) => {
            w.write_ref(s, f);
            crate::local::write_names_around(w, s, f);
            crate::local::write_signature_names(w, s, f);
        }
        Group::Holder(h) => {
            s.uv(REF_NONE).count(0);
            w.write_ref_opt(s, h);
            match h {
                Some(h) => crate::local::write_names_around(w, s, h),
                None => {
                    s.count(0);
                }
            }
        }
    }
    s.uv(n);
    match file {
        Some(lit) => {
            s.opt(true);
            string(w, s, lit);
        }
        None => {
            s.opt(false);
        }
    }
}

/// The memo of content digests for the walk's mode: one written with
/// name-derived values by value ([`Walk::by_value`]) differs from one
/// written with them symbolically.
fn memo<'w>(w: &'w Walk<'_>) -> &'w HashMap<u32, Hashed> {
    if w.by_value > 0 { &w.const_memo_by_value } else { &w.const_memo }
}

/// [`memo`], to insert into.
fn memo_mut<'w>(w: &'w mut Walk<'_>) -> &'w mut HashMap<u32, Hashed> {
    if w.by_value > 0 { &mut w.const_memo_by_value } else { &mut w.const_memo }
}

/// The content digest of constant `c` (`PtrBase/Const.0`): from the
/// walk's memo, from this call's, or computed with every constant it
/// reaches ([`compute`]).
fn const_digest(w: &mut Walk<'_>, cx: &mut Ctx, c: ConstValueId) -> Hashed {
    let i = c.index();
    if let Some(&h) = memo(w).get(&i) {
        return h;
    }
    if let Some(&h) = cx.local.get(&i) {
        cx.tainted = true;
        return h;
    }
    if cx.computing {
        // Unreachable with Kiln's store (a value names earlier values
        // only, and those are computed first).
        return Hashed { digest: Digest([0; 32]), cause: Some(OUT_OF_ORDER), bytes: 0 };
    }
    if !compute(w, cx, c) {
        cx.tainted = true;
        return Hashed { digest: Digest([0; 32]), cause: Some(TOO_DEEP), bytes: 0 };
    }
    if let Some(&h) = memo(w).get(&i) {
        return h;
    }
    if let Some(&h) = cx.local.get(&i) {
        cx.tainted = true;
        return h;
    }
    Hashed { digest: Digest([0; 32]), cause: Some(OUT_OF_ORDER), bytes: 0 }
}

/// Computes the content digest of `root` and of every constant it reaches
/// that has none yet, in increasing id order (children first), with the
/// unit's local state put aside. `false` past the depth limit.
fn compute(w: &mut Walk<'_>, cx: &mut Ctx, root: ConstValueId) -> bool {
    if !w.enter() {
        return false;
    }
    // The constants to compute: a walk over the DAG with an explicit
    // stack, skipping those already known.
    let mut todo: Vec<ConstValueId> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        let i = v.index();
        if memo(w).contains_key(&i) || cx.local.contains_key(&i) || !seen.insert(i) {
            continue;
        }
        todo.push(v);
        children(w, v, &mut stack);
    }
    todo.sort_by_key(|v| v.index());
    let saved_unit = std::mem::take(&mut w.unit);
    w.in_key += 1;
    let (budget, tainted) = (cx.budget, cx.tainted);
    cx.computing = true;
    for v in todo {
        cx.budget = VALUE_BUDGET;
        cx.tainted = false;
        let (aborts, rooted) = (w.aborts, w.local.self_rooted);
        let mut s = Stream::new();
        write_value(w, &mut s, v, cx);
        let h = w.hashed(s.finish(VALUE_TAG));
        if cx.tainted || w.aborts != aborts || w.local.self_rooted != rooted {
            cx.local.insert(v.index(), h);
        } else {
            memo_mut(w).insert(v.index(), h);
        }
    }
    cx.computing = false;
    (cx.budget, cx.tainted) = (budget, tainted);
    w.in_key -= 1;
    w.unit = saved_unit;
    w.leave();
    true
}

/// The constants whose content digest the stream of `v` writes: the
/// `PtrBase::Const` of `v` if it is a pointer, of each relocation if it is
/// an image (a relocation is written in place).
fn children(w: &Walk<'_>, v: ConstValueId, out: &mut Vec<ConstValueId>) {
    match w.ast.consts.get(v) {
        ConstValue::Scalar(x) => scalar_child(&x, out),
        ConstValue::Aggregate(ByteImage { ty: _, bytes: _, relocs, padding: _, unions: _ }) => {
            for &Reloc { at: _, value } in relocs {
                match w.ast.consts.get(value) {
                    ConstValue::Scalar(x) => scalar_child(&x, out),
                    // Not pointer-like: `push_aggregate` rejects it, and
                    // its stream would write it in place.
                    ConstValue::Aggregate(_) => out.push(value),
                }
            }
        }
    }
}

/// The constant a scalar's base names, if any.
fn scalar_child(x: &Scalar<'_>, out: &mut Vec<ConstValueId>) {
    match *x {
        Scalar::Pointer { ty: _, base, offset: _, designated: _, past: _ } => match base {
            PtrBase::Const(c) => out.push(c),
            PtrBase::Decl(_)
            | PtrBase::Function(_)
            | PtrBase::Temporary(_)
            | PtrBase::StringLit { lit: _, origin: _ }
            | PtrBase::TypeInfo(_)
            | PtrBase::Label { label: _, origin: _ }
            | PtrBase::Absolute
            | PtrBase::LabelDiff { plus: _, minus: _, size: _ } => {}
        },
        Scalar::Int { ty: _, signed: _, bits: _ }
        | Scalar::Bool { ty: _, value: _ }
        | Scalar::Float { ty: _, kind: _, bits: _ }
        | Scalar::Null { ty: _, kind: _ }
        | Scalar::MemberPtr { ty: _, member: _, kind: _, derived: _, path: _ }
        | Scalar::Reflection { ty: _, id: _ } => {}
    }
}

/// `Temporary.0`: the `Ref` of the declaration that extends the
/// temporary, then its `LetData` `mangling_index` (digest §7).
fn write_temporary(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    match ast.decls.lifetime_extended(d) {
        Some(&LetData { extending, mangling_index, value: _ }) => {
            w.write_ref(s, extending);
            s.uv(u64::from(mangling_index));
        }
        None => {
            s.uv(REF_NONE).uv(0).indeterminate(Cause::Unaccounted("PtrBase/Temporary.0"));
        }
    }
}

/// The label an `AddrLabelExpr` names, as a `ref` (`LabelDiff.plus`,
/// `LabelDiff.minus`).
fn write_addr_label(w: &mut Walk<'_>, s: &mut Stream, e: ExprId, row: &'static str) {
    let ast = w.ast;
    if ast.exprs.get(e).kind() == ExprKind::AddrLabelExpr {
        w.write_ref(s, ast.exprs.addr_label(e));
    } else {
        s.uv(REF_NONE).indeterminate(Cause::Unaccounted(row));
    }
}

/// `record ByteImage`, after the `Aggregate` tag (digest §7 last rule).
fn write_image(w: &mut Walk<'_>, s: &mut Stream, img: ByteImage<'_>, cx: &mut Ctx) {
    let ByteImage { ty, bytes, relocs, padding, unions } = img;
    variant(s, value_kind::AGGREGATE, "ValueKind/Aggregate");
    w.write_type(s, ty);
    // `bytes`: as stored; Kiln zeroes padding and relocated bytes (the
    // module documentation says where).
    if spend(s, cx, bytes.len() as u64) {
        s.bytes(bytes);
    } else {
        s.bytes(&[]);
    }
    s.count(relocs.len());
    for &Reloc { at, value } in relocs {
        s.uv(u64::from(at));
        write_value(w, s, value, cx);
    }
    s.count(padding.len());
    for &ByteRange { start, len } in padding {
        s.uv(u64::from(start)).uv(u64::from(len));
    }
    s.count(unions.len());
    for &UnionMember { at, field } in unions {
        s.uv(u64::from(at));
        w.write_ref(s, field);
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests of the rows that only a variable's evaluated value or a
    //! fold reaches (WP-028): `value` is called on a constant of a parsed
    //! snippet's store, or of a store built by hand, and its bytes are
    //! compared with the rows written one by one.

    use std::fmt::Write as _;
    use std::sync::Arc;

    use slip_kiln::kiln::ast::consts::{
        ConstValue, FloatKind, IntBits, MemberKind, NullKind, PtrBase, Scalar,
    };
    use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, DeclMisc, NewDecl, Payload};
    use slip_kiln::kiln::ast::names::DeclName;
    use slip_kiln::kiln::ast::strings::StringEncoding;
    use slip_kiln::kiln::ast::types::TypeContext;
    use slip_kiln::kiln::ast::{Ast, ConstValueId, DeclId, QualType};
    use slip_kiln::kiln::source::Loc;
    use slip_kiln::kiln::source::SourceManager;
    use slip_kiln::kiln::source::symbol::Interner;
    use slip_model::keys::RuleTable;

    use slip_model::Digest;

    use super::{Ctx, TOO_LARGE, VALUE_TAG, string, value, write_value};
    use crate::refs::REF_LOCAL;
    use crate::stream::Stream;
    use crate::tables::{
        float_kind, member_kind, null_kind, ptr_base, string_encoding, value_kind,
    };
    use crate::walk::{DEPTH_LIMIT, TOO_DEEP, Walk};
    use crate::{Cause, Input};

    /// Runs `f` on a walk over the snippet `src` in language mode `std`.
    fn with_walk<R>(src: &str, std: &str, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let files = [("main.cpp", src)];
        let snippet =
            slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the snippet parses")
    }

    /// Runs `f` on a walk over a hand-built `ast`.
    fn with_ast<R>(ast: &Ast, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let sm = SourceManager::new();
        let rules = RuleTable::empty();
        let path = |_| Vec::new();
        let input = Input { ast, sm: &sm, subject_path: &path };
        let mut w = Walk::new(&input, &rules);
        f(&mut w)
    }

    fn written(w: &mut Walk<'_>, v: ConstValueId) -> Stream {
        let mut s = Stream::new();
        value(w, &mut s, v);
        s
    }

    /// Every constant of the store for which `pick` holds, in id order.
    fn find(w: &Walk<'_>, pick: impl Fn(&ConstValue<'_>) -> bool) -> Vec<ConstValueId> {
        w.ast.consts.ids().filter(|&v| pick(&w.ast.consts.get(v))).collect()
    }

    /// The rows of `Pointer` after its base: `offset`, `designated`, `past`
    /// for a pointer at offset 0 with no designated type.
    fn plain_tail(s: &mut Stream) {
        s.uv(0).opt(false).bool(false);
    }

    // digest-kinds §8 `Scalar` `Int.bits`: the normalised words, each
    // little-endian, 8 bytes per 64 bits; `Int.ty`, `Int.signed` before.
    #[test]
    fn int_bits_are_the_normalised_words() {
        let src = "template <auto V> struct A;\nA<(unsigned __int128)1 << 100>* x;\nA<-1>* y;\n";
        with_walk(src, "c++20", |w| {
            let v = find(
                w,
                |c| matches!(c, ConstValue::Scalar(Scalar::Int { bits, .. }) if bits.width() == 128),
            );
            let mut want = Stream::new();
            want.tag(value_kind::INT);
            w.write_type(&mut want, QualType::UINT128);
            let mut b = [0u8; 16];
            b[12] = 0x10;
            want.bool(false).bytes(&b);
            assert_eq!(written(w, v[0]).as_bytes(), want.as_bytes());
            let v = find(
                w,
                |c| matches!(c, ConstValue::Scalar(Scalar::Int { signed: true, bits, .. }) if bits.width() == 32 && bits.to_i128() == Some(-1)),
            );
            // A 32-bit value is one word: eight bytes, the high four zero.
            let mut eight = [0u8; 8];
            eight[..4].copy_from_slice(&[0xff; 4]);
            let mut want8 = Stream::new();
            want8.tag(value_kind::INT);
            w.write_type(&mut want8, QualType::INT);
            want8.bool(true).bytes(&eight);
            assert_eq!(written(w, v[0]).as_bytes(), want8.as_bytes());
        });
    }

    // digest-kinds §8 `Scalar` `Float.kind`, `Float.bits`: the exact
    // encoding in 16 bytes; `-0.0` is not `0.0`.
    #[test]
    fn float_bits_are_exact() {
        let src = "template <auto V> struct A;\nA<-0.0>* x;\n";
        with_walk(src, "c++20", |w| {
            let v = find(
                w,
                |c| matches!(c, ConstValue::Scalar(Scalar::Float { bits, .. }) if *bits == 1 << 63),
            );
            let mut want = Stream::new();
            want.tag(value_kind::FLOAT);
            w.write_type(&mut want, QualType::DOUBLE);
            want.tag(float_kind::DOUBLE).bytes(&(1u128 << 63).to_le_bytes());
            assert_eq!(written(w, v[0]).as_bytes(), want.as_bytes());
        });
    }

    // digest-kinds §10.3 `Entry` (`start`, then `enc`), every
    // `StringEncoding` the target has, §8 `PtrBase` `StringLit` (`origin`
    // out: two equal literals are compared by content, digest §7).
    #[test]
    fn string_literals_by_content_and_encoding() {
        let src = "constexpr const char* o = \"ab\";\nconstexpr const char8_t* u8 = u8\"ab\";\n\
                   constexpr const char16_t* u16 = u\"ab\";\nconstexpr const char32_t* u32 = U\"ab\";\n\
                   constexpr const wchar_t* wd = L\"ab\";\nconstexpr const char* o2 = \"ab\";\n";
        with_walk(src, "c++20", |w| {
            let ptrs = find(w, |c| {
                matches!(
                    c,
                    ConstValue::Scalar(Scalar::Pointer { base: PtrBase::StringLit { .. }, .. })
                )
            });
            assert_eq!(ptrs.len(), 6, "one pointer per literal expression");
            let mut seen = Vec::new();
            let mut bytes = Vec::new();
            for &v in &ptrs {
                let ConstValue::Scalar(Scalar::Pointer {
                    ty,
                    base: PtrBase::StringLit { lit, .. },
                    ..
                }) = w.ast.consts.get(v)
                else {
                    unreachable!("picked above")
                };
                let (enc, units) = w.ast.strings.get(lit);
                let mut want = Stream::new();
                want.tag(value_kind::POINTER);
                w.write_type(&mut want, ty);
                want.tag(ptr_base::STRING_LIT).bytes(units).tag(string_encoding::row(&enc));
                plain_tail(&mut want);
                let got = written(w, v);
                assert_eq!(got.as_bytes(), want.as_bytes(), "{enc:?}");
                assert_eq!(got.cause(), None);
                seen.push(enc);
                bytes.push(got.as_bytes().to_vec());
            }
            assert_eq!(
                seen,
                [
                    StringEncoding::Ordinary,
                    StringEncoding::Utf8,
                    StringEncoding::Utf16,
                    StringEncoding::Utf32,
                    StringEncoding::Wide32,
                    StringEncoding::Ordinary,
                ]
            );
            assert_eq!(bytes[0], bytes[5], "\"ab\" twice: one content");
            assert_ne!(bytes[0], bytes[1], "the same code units, another encoding");
            // `string` alone: the code units, then the encoding's tag.
            let (enc, units) = w.ast.strings.get(match w.ast.consts.get(ptrs[2]) {
                ConstValue::Scalar(Scalar::Pointer {
                    base: PtrBase::StringLit { lit, .. },
                    ..
                }) => lit,
                _ => unreachable!("picked above"),
            });
            assert_eq!(enc, StringEncoding::Utf16);
            assert_eq!(units, b"a\0b\0");
        });
    }

    // digest-kinds §8 `PtrBase` `Temporary.0`, digest §7: the `Ref` of the
    // extending declaration and the temporary's index among its, not the
    // temporary's value.
    #[test]
    fn a_temporary_is_its_extending_declaration_and_index() {
        let src = "struct P { const int& a; const int& b; };\nconstexpr P pp{1, 2};\n";
        with_walk(src, "c++20", |w| {
            let ptrs = find(w, |c| {
                matches!(c, ConstValue::Scalar(Scalar::Pointer { base: PtrBase::Temporary(_), .. }))
            });
            assert_eq!(ptrs.len(), 2);
            let mut out = Vec::new();
            for &v in &ptrs {
                let ConstValue::Scalar(Scalar::Pointer { ty, base: PtrBase::Temporary(t), .. }) =
                    w.ast.consts.get(v)
                else {
                    unreachable!("picked above")
                };
                let l = *w.ast.decls.lifetime_extended(t).expect("a LetData");
                let mut want = Stream::new();
                want.tag(value_kind::POINTER);
                w.write_type(&mut want, ty);
                want.tag(ptr_base::TEMPORARY);
                w.write_ref(&mut want, l.extending);
                want.uv(u64::from(l.mangling_index));
                plain_tail(&mut want);
                let got = written(w, v);
                assert_eq!(got.as_bytes(), want.as_bytes());
                out.push((l.mangling_index, got.as_bytes().to_vec()));
            }
            out.sort();
            assert_eq!((out[0].0, out[1].0), (0, 1));
            assert_ne!(out[0].1, out[1].1, "two temporaries of one declaration differ by index");
        });
    }

    // digest-kinds §8 `PtrBase` `TypeInfo.0`: the type digest.
    #[test]
    fn type_info_is_its_type() {
        let src = "namespace std { class type_info; }\n\
                   constexpr const std::type_info* a = &typeid(int);\n\
                   constexpr const std::type_info* b = &typeid(char);\n";
        with_walk(src, "c++20", |w| {
            let ptrs = find(w, |c| {
                matches!(c, ConstValue::Scalar(Scalar::Pointer { base: PtrBase::TypeInfo(_), .. }))
            });
            assert_eq!(ptrs.len(), 2);
            let mut all = Vec::new();
            for &v in &ptrs {
                let ConstValue::Scalar(Scalar::Pointer { ty, base: PtrBase::TypeInfo(q), .. }) =
                    w.ast.consts.get(v)
                else {
                    unreachable!("picked above")
                };
                let mut want = Stream::new();
                want.tag(value_kind::POINTER);
                w.write_type(&mut want, ty);
                want.tag(ptr_base::TYPE_INFO);
                w.write_type(&mut want, q);
                plain_tail(&mut want);
                let got = written(w, v);
                assert_eq!(got.as_bytes(), want.as_bytes());
                all.push(got.as_bytes().to_vec());
            }
            assert_ne!(all[0], all[1]);
        });
    }

    // digest-kinds §8 `PtrBase` `Label.label` (`origin` out): a label is
    // its local ordinal (digest §4.3), first met first.
    #[test]
    fn a_label_is_its_local_ordinal() {
        let src = "void f() { l: m: constexpr void* p = &&m; constexpr void* q = &&l; \
                   constexpr void* r = &&m; }\n";
        with_walk(src, "c++20", |w| {
            let ptrs = find(w, |c| {
                matches!(c, ConstValue::Scalar(Scalar::Pointer { base: PtrBase::Label { .. }, .. }))
            });
            assert_eq!(ptrs.len(), 3, "one value per `&&` expression");
            let ty = w.ast.consts.ty(ptrs[0]);
            for (v, n) in [(ptrs[0], 0), (ptrs[1], 1), (ptrs[2], 0)] {
                let mut want = Stream::new();
                want.tag(value_kind::POINTER);
                w.write_type(&mut want, ty);
                want.tag(ptr_base::LABEL).uv(REF_LOCAL).uv(n);
                plain_tail(&mut want);
                assert_eq!(written(w, v).as_bytes(), want.as_bytes(), "ordinal {n}");
            }
        });
    }

    /// A store built by hand: `Absolute` and `LabelDiff` come only from a C
    /// static initializer's fold, which no C++ snippet reaches.
    fn hand_built() -> (Ast, QualType) {
        let mut ast = Ast::new(Arc::new(Interner::new()), TypeContext::default());
        let void_ptr = ast.types.pointer_to(QualType::VOID);
        (ast, void_ptr)
    }

    fn label(ast: &mut Ast) -> DeclId {
        let tu = ast.tu;
        ast.decls.push(
            NewDecl {
                kind: DeclKind::Label,
                flags: DeclFlags::empty(),
                misc: DeclMisc::default(),
                name: DeclName::EMPTY,
                ctx: Some(tu),
                loc: Loc::INVALID,
                ty: None,
            },
            Payload::None(0),
        )
    }

    // digest-kinds §8 `PtrBase` `Absolute`: the tag; the address is the
    // pointer's `offset`.
    #[test]
    fn an_absolute_address_is_its_offset() {
        let (mut ast, ty) = hand_built();
        let mk = |offset| Scalar::Pointer {
            ty,
            base: PtrBase::Absolute,
            offset,
            designated: None,
            past: false,
        };
        let a = ast.consts.push_scalar(mk(16)).expect("a pointer");
        let b = ast.consts.push_scalar(mk(32)).expect("a pointer");
        with_ast(&ast, |w| {
            let mut want = Stream::new();
            want.tag(value_kind::POINTER);
            w.write_type(&mut want, ty);
            want.tag(ptr_base::ABSOLUTE).uv(16).opt(false).bool(false);
            let got = written(w, a);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_eq!(got.cause(), None);
            assert_ne!(got.as_bytes(), written(w, b).as_bytes());
        });
    }

    // digest-kinds §8 `PtrBase` `LabelDiff.plus`, `.minus` (the labels the
    // `AddrLabelExpr`s name) and `.size`.
    #[test]
    fn a_label_difference_is_its_labels_and_size() {
        let (mut ast, void_ptr) = hand_built();
        let (l, m) = (label(&mut ast), label(&mut ast));
        let plus = ast.builder().addr_label(l, void_ptr, Loc::INVALID, Loc::INVALID);
        let minus = ast.builder().addr_label(m, void_ptr, Loc::INVALID, Loc::INVALID);
        let mk = |plus, minus, size| Scalar::Pointer {
            ty: QualType::LONG,
            base: PtrBase::LabelDiff { plus, minus, size },
            offset: 0,
            designated: None,
            past: false,
        };
        let lm = ast.consts.push_scalar(mk(plus, minus, 8)).expect("a difference");
        let ml = ast.consts.push_scalar(mk(minus, plus, 8)).expect("a difference");
        let lm4 = ast.consts.push_scalar(mk(plus, minus, 4)).expect("a difference");
        with_ast(&ast, |w| {
            // `m` is seen first, so its ordinal is 0: a swap of `plus` and
            // `minus` shows in the ordinals.
            w.write_ref(&mut Stream::new(), m);
            let mut want = Stream::new();
            want.tag(value_kind::POINTER);
            w.write_type(&mut want, QualType::LONG);
            want.tag(ptr_base::LABEL_DIFF).uv(REF_LOCAL).uv(1).uv(REF_LOCAL).uv(0).uv(8);
            plain_tail(&mut want);
            let got = written(w, lm);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_eq!(got.cause(), None);
            assert_ne!(got.as_bytes(), written(w, ml).as_bytes(), "plus and minus swapped");
            assert_ne!(got.as_bytes(), written(w, lm4).as_bytes(), "another size");
        });
    }

    // spec/digest.md §7, digest-kinds §8: nothing written depends on a
    // `ConstValueId` or on interning order.
    #[test]
    fn values_do_not_depend_on_interning_order() {
        let one = |src: &str| {
            with_walk(src, "c++20", |w| {
                let v = find(w, |c| matches!(c, ConstValue::Aggregate(_)));
                assert_eq!(v.len(), 1);
                written(w, v[0]).as_bytes().to_vec()
            })
        };
        let pre =
            "template <auto V> struct A;\nint g, h;\nstruct P { const int* p; const int* q; };\n";
        let a = one(&format!("{pre}A<&h>* y;\nA<&g>* z;\nA<P{{&g, &h}}>* x;\n"));
        let b = one(&format!("{pre}A<P{{&g, &h}}>* x;\n"));
        assert_eq!(a, b);
    }

    // spec/digest.md §7 last rule, digest-kinds §8 `ByteImage`: type,
    // bytes, relocations (each the pointer it holds), padding, active
    // union members, in that order.
    #[test]
    fn an_aggregate_by_its_rows() {
        let src = "template <auto V> struct A;\nint g;\n\
                   struct P { char c; const int* p; union { int i; char d; }; };\nA<P{1, &g, {.d = 2}}>* x;\n";
        with_walk(src, "c++20", |w| {
            let v = find(w, |c| matches!(c, ConstValue::Aggregate(_)))[0];
            let ConstValue::Aggregate(img) = w.ast.consts.get(v) else { unreachable!("picked") };
            let mut want = Stream::new();
            want.tag(value_kind::AGGREGATE);
            w.write_type(&mut want, img.ty);
            want.bytes(img.bytes).count(1).uv(8);
            let mut ptr = Stream::new();
            value(w, &mut ptr, img.relocs[0].value);
            want.raw(ptr.as_bytes());
            want.count(img.padding.len());
            for r in img.padding {
                want.uv(u64::from(r.start)).uv(u64::from(r.len));
            }
            want.count(1).uv(16);
            w.write_ref(&mut want, img.unions[0].field);
            assert_eq!(img.padding.len(), 2, "after `c`, and the bytes `d` leaves");
            assert_eq!(written(w, v).as_bytes(), want.as_bytes());
        });
    }

    // digest-kinds §8 `ValueKind` `Reflection` (unsupported, §12): the tag
    // only, and the stream is indeterminate.
    #[test]
    fn a_reflection_is_unsupported() {
        with_walk("template <auto V> struct A;\nA<^^int>* x;\n", "c++26", |w| {
            let v = find(w, |c| matches!(c, ConstValue::Scalar(Scalar::Reflection { .. })))[0];
            let got = written(w, v);
            let mut want = Stream::new();
            want.tag(value_kind::REFLECTION);
            assert_eq!(got.as_bytes(), want.as_bytes());
            assert_eq!(got.cause(), Some(Cause::Unsupported("ValueKind/Reflection")));
        });
    }

    // digest-kinds §8 `Scalar` `Null.kind` (`NullKind`) and `MemberPtr`
    // `kind` (`MemberKind`), `derived`: each by its row, with the type held.
    #[test]
    fn null_and_member_kinds_by_their_rows() {
        let (mut ast, ty) = hand_built();
        let member = label(&mut ast);
        let mut null = |kind| ast.consts.push_scalar(Scalar::Null { ty, kind }).expect("a null");
        let nulls =
            [NullKind::Pointer, NullKind::MemberData, NullKind::MemberFunction].map(&mut null);
        let mut mp = |kind, derived| {
            let path: &[DeclId] = &[];
            ast.consts
                .push_scalar(Scalar::MemberPtr { ty, member, kind, derived, path })
                .expect("a member pointer")
        };
        let data = mp(MemberKind::Data, false);
        let function = mp(MemberKind::Function, false);
        let derived = mp(MemberKind::Data, true);
        with_ast(&ast, |w| {
            for (v, kind) in nulls.into_iter().zip([
                NullKind::Pointer,
                NullKind::MemberData,
                NullKind::MemberFunction,
            ]) {
                let mut want = Stream::new();
                want.tag(value_kind::NULL);
                w.write_type(&mut want, ty);
                want.tag(null_kind::row(&kind));
                assert_eq!(written(w, v).as_bytes(), want.as_bytes());
            }
            w.write_ref(&mut Stream::new(), member);
            let mut want = Stream::new();
            want.tag(value_kind::MEMBER_PTR);
            w.write_type(&mut want, ty);
            want.uv(REF_LOCAL).uv(0).tag(member_kind::FUNCTION).bool(false).count(0);
            assert_eq!(written(w, function).as_bytes(), want.as_bytes());
            assert_ne!(written(w, data).as_bytes(), want.as_bytes(), "another kind");
            let d = written(w, derived);
            assert_ne!(d.as_bytes(), written(w, data).as_bytes(), "derived");
            assert_eq!(d.cause(), None);
        });
    }

    // WP-028 (`spec/landed/digest-kinds/WP-028.md`): one constant's stream
    // does at most its budget of work in place; past it, `value/size`.
    #[test]
    fn work_is_bounded() {
        let src = "template <auto V> struct A;\nint g;\nstruct P { const int* p; int a[8]; };\nA<P{&g}>* x;\n";
        with_walk(src, "c++20", |w| {
            let v = find(w, |c| matches!(c, ConstValue::Aggregate(_)))[0];
            let with_budget = |w: &mut Walk<'_>, budget| {
                let mut cx = Ctx::new();
                cx.budget = budget;
                let mut s = Stream::new();
                write_value(w, &mut s, v, &mut cx);
                (s.cause(), cx.budget)
            };
            assert_eq!(with_budget(w, 1 + 40 + 1), (None, 0), "the image and its pointer fit");
            assert_eq!(with_budget(w, 1 + 40).0, Some(TOO_LARGE));
            assert_eq!(with_budget(w, 1 + 39).0, Some(TOO_LARGE));
        });
    }

    /// The bytes of the stream of `c` written in place, and their content
    /// digest (`PtrBase/Const.0`).
    fn content(w: &mut Walk<'_>, c: ConstValueId) -> Digest {
        let mut s = Stream::new();
        write_value(w, &mut s, c, &mut Ctx::new());
        s.finish(VALUE_TAG).digest
    }

    // digest §7, `spec/landed/digest-kinds/WP-028.md`: a pointer to a
    // constant writes that constant's content digest; a memoised digest
    // gives the bytes a fresh walk gives.
    #[test]
    fn a_pointer_to_a_constant_is_its_content() {
        let (mut ast, ty) = hand_built();
        let int = ast
            .consts
            .push_scalar(Scalar::Int {
                ty: QualType::INT,
                signed: true,
                bits: IntBits::from_i128(32, 7),
            })
            .expect("an int");
        let ptr = |ast: &mut Ast, c| {
            ast.consts
                .push_scalar(Scalar::Pointer {
                    ty,
                    base: PtrBase::Const(c),
                    offset: 0,
                    designated: None,
                    past: false,
                })
                .expect("a pointer")
        };
        let p1 = ptr(&mut ast, int);
        let p2 = ptr(&mut ast, p1);
        let fresh = with_ast(&ast, |w| written(w, p1).as_bytes().to_vec());
        with_ast(&ast, |w| {
            let top = written(w, p2);
            assert_eq!(top.cause(), None);
            assert!(w.const_memo.contains_key(&p1.index()), "memoised");
            assert_eq!(written(w, p1).as_bytes(), fresh, "memoised as a fresh walk");
            let mut want = Stream::new();
            want.tag(value_kind::POINTER);
            w.write_type(&mut want, ty);
            want.tag(ptr_base::CONST).digest(&content(w, p1));
            plain_tail(&mut want);
            assert_eq!(top.as_bytes(), want.as_bytes());
        });
    }

    // `spec/landed/digest-kinds/WP-028.md`: a content digest is computed as
    // a key is, with the unit put aside: a local entity met below it is
    // `key/local-ref`, and the unit's ordinals are untouched.
    #[test]
    fn a_content_digest_does_not_depend_on_the_unit() {
        let (mut ast, ty) = hand_built();
        let l = label(&mut ast);
        let at = |ast: &mut Ast, base| {
            ast.consts
                .push_scalar(Scalar::Pointer { ty, base, offset: 0, designated: None, past: false })
                .expect("a pointer")
        };
        let inner = at(&mut ast, PtrBase::Decl(l));
        let outer = at(&mut ast, PtrBase::Const(inner));
        with_ast(&ast, |w| {
            assert_eq!(written(w, outer).cause(), Some(Cause::Unaccounted("key/local-ref")));
            assert!(w.unit.locals.is_empty(), "no ordinal taken");
            assert_eq!(w.in_key, 0);
        });
    }

    // WP-028 review: constants whose pointers share a constant are written
    // once each, not once per path: a 30-level chain whose levels each
    // hold two pointers to the level below hashes a few kilobytes.
    #[test]
    fn shared_constants_are_written_once() {
        let mut src = String::from(
            "namespace std::meta { using info = decltype(^^int); template <class T> consteval T extract(info r);\n\
             template <class R> consteval info __kiln_reflect_constant_array(R&& r); }\n\
             template <auto V> struct A;\nconstexpr int a0[] = {1, 0};\n\
             constexpr const int* p0 = std::meta::extract<const int*>(std::meta::__kiln_reflect_constant_array(a0));\n",
        );
        let n = 30;
        for k in 1..=n {
            writeln!(
                src,
                "constexpr const void* a{k}[] = {{p{j}, p{j}}};\n\
                 constexpr const void* const* p{k} = std::meta::extract<const void* const*>(std::meta::__kiln_reflect_constant_array(a{k}));",
                j = k - 1
            )
            .expect("a string");
        }
        writeln!(src, "A<p{n}>* x;").expect("a string");
        with_walk(&src, "c++26", |w| {
            let top = w.ast.consts.ids().last().expect("constants");
            let got = written(w, top);
            assert_eq!(got.cause(), None);
            assert!(w.bytes < 100_000, "linear: {} bytes hashed", w.bytes);
        });
    }

    // lib.rs "What bounds the stack": a chain of pointers to constants is
    // computed children first, so its length is not stack depth; a value
    // met past the walk's depth limit is `walk/depth`, and that digest is
    // not memoised (its cause depends on the path).
    #[test]
    fn nesting_is_bounded() {
        let (mut ast, ty) = hand_built();
        let mut chain = |n: usize| {
            let mut v = ast.consts.push_scalar(Scalar::Int {
                ty: QualType::INT,
                signed: true,
                bits: IntBits::from_i128(32, 7),
            });
            for _ in 0..n {
                let base = PtrBase::Const(v.expect("a value"));
                v = ast.consts.push_scalar(Scalar::Pointer {
                    ty,
                    base,
                    offset: 0,
                    designated: None,
                    past: false,
                });
            }
            v.expect("a value")
        };
        let (deep, deeper) = (chain(250), chain(300));
        with_ast(&ast, |w| {
            // From two levels below the limit, the value enters one and the
            // computation of its content digests the last: each constant's
            // own stream is cut there.
            w.depth = DEPTH_LIMIT - 2;
            for v in [deep, deeper] {
                assert_eq!(written(w, v).cause(), Some(TOO_DEEP));
                assert_eq!(w.depth, DEPTH_LIMIT - 2, "every level entered is left");
            }
            assert!(w.const_memo.is_empty(), "nothing below a depth hit is memoised");
            w.depth = 0;
            let (a, b) = (written(w, deep), written(w, deeper));
            assert_eq!((a.cause(), b.cause()), (None, None));
            assert_ne!(a.as_bytes(), b.as_bytes());
            assert_eq!(w.depth, 0);
        });
    }

    // digest-kinds §8 `Scalar` `Float.kind`: the row of each `FloatKind`
    // the target builds, with `Float.bits`.
    #[test]
    fn every_float_kind_by_its_row() {
        let src = "template <auto V> struct A;\nA<1.0f>* a;\nA<1.0>* b;\nA<1.0L>* c;\n\
                   A<(__float128)1.0>* d;\nA<(_Float16)1.0>* e;\nA<(__bf16)1.0>* f;\n";
        with_walk(src, "c++20", |w| {
            let floats = find(w, |c| matches!(c, ConstValue::Scalar(Scalar::Float { .. })));
            let mut kinds = Vec::new();
            for v in floats {
                let ConstValue::Scalar(Scalar::Float { ty, kind, bits }) = w.ast.consts.get(v)
                else {
                    unreachable!("picked above")
                };
                let mut want = Stream::new();
                want.tag(value_kind::FLOAT);
                w.write_type(&mut want, ty);
                want.tag(float_kind::row(&kind)).bytes(&bits.to_le_bytes());
                assert_eq!(written(w, v).as_bytes(), want.as_bytes(), "{kind:?}");
                kinds.push(kind);
            }
            assert_eq!(
                kinds,
                [
                    FloatKind::Single,
                    FloatKind::Double,
                    FloatKind::X87Extended,
                    FloatKind::Quad,
                    FloatKind::Half,
                    FloatKind::BFloat16
                ]
            );
        });
    }

    // digest-kinds §8 `Scalar` `Int.ty`, `Pointer.ty`: the type is written
    // when nothing else differs.
    #[test]
    fn the_type_of_a_scalar_by_its_row() {
        let (mut ast, void_ptr) = hand_built();
        let int_ptr = ast.types.pointer_to(QualType::INT);
        let mut int = |ty| {
            ast.consts
                .push_scalar(Scalar::Int { ty, signed: true, bits: IntBits::from_i128(32, 1) })
                .expect("an int")
        };
        let (i, u) = (int(QualType::INT), int(QualType::UINT));
        let mut ptr = |ty| {
            ast.consts
                .push_scalar(Scalar::Pointer {
                    ty,
                    base: PtrBase::Absolute,
                    offset: 8,
                    designated: None,
                    past: false,
                })
                .expect("a pointer")
        };
        let (p, q) = (ptr(void_ptr), ptr(int_ptr));
        with_ast(&ast, |w| {
            assert_ne!(written(w, i).as_bytes(), written(w, u).as_bytes(), "Int.ty");
            assert_ne!(written(w, p).as_bytes(), written(w, q).as_bytes(), "Pointer.ty");
        });
    }

    // digest-kinds §10.3 `Entry.enc`: the same code-unit bytes in two
    // encodings differ by the tag, for every `StringEncoding` (`Wide16`,
    // which this target's `wchar_t` is not, built by hand).
    #[test]
    fn an_encoding_by_its_row() {
        let (mut ast, _) = hand_built();
        let encs = [
            StringEncoding::Ordinary,
            StringEncoding::Utf8,
            StringEncoding::Utf16,
            StringEncoding::Utf32,
            StringEncoding::Wide16,
            StringEncoding::Wide32,
        ];
        let lits = encs.map(|e| ast.strings.intern(e, b"abcd"));
        with_ast(&ast, |w| {
            let mut all = Vec::new();
            for (l, e) in lits.into_iter().zip(encs) {
                let mut got = Stream::new();
                string(w, &mut got, l);
                let mut want = Stream::new();
                want.bytes(b"abcd").tag(string_encoding::row(&e));
                assert_eq!(got.as_bytes(), want.as_bytes(), "{e:?}");
                all.push(got.as_bytes().to_vec());
            }
            all.sort();
            all.dedup();
            assert_eq!(all.len(), encs.len(), "ordinary is not UTF-8, nor Wide16 UTF-16");
        });
    }
}

#[cfg(test)]
mod name_value_tests {
    //! The identity of the source-location objects (digest §9, WP-115).

    use slip_kiln::kiln::ast::DeclId;
    use slip_model::keys::RuleTable;

    use super::{Group, name_values};
    use crate::Input;
    use crate::walk::Walk;

    const SRCLOC: &str = "namespace std {
struct source_location {
  struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; };
  static consteval source_location current(const void *p = __builtin_source_location()) noexcept {
    source_location s; s.impl = static_cast<const __impl *>(p); return s; }
  constexpr unsigned line() const noexcept { return impl ? impl->_M_line : 0; }
  const __impl *impl = nullptr;
};
}
";

    /// Runs `f` on a walk over `src` after [`SRCLOC`], C++20.
    fn with_walk<R>(src: &str, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let text = format!("{SRCLOC}{src}");
        let files = [("main.cpp", text.as_str())];
        let snippet = slip_kiln::snippet::Snippet {
            files: &files,
            main: "main.cpp",
            include: &[""],
            std: "c++20",
        };
        let rules = RuleTable::empty();
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the snippet parses")
    }

    /// The name of `d` as Kiln prints it.
    fn name(w: &Walk<'_>, d: DeclId) -> String {
        let names = slip_kiln::kiln::ast::dump::AstNames::with_sources(w.ast, w.sm);
        let mut t = String::new();
        names.write_name(d, &mut t);
        t
    }

    /// Every `__impl` object's group and ordinal, sorted, as text.
    fn objects(w: &mut Walk<'_>) -> Vec<String> {
        let all: Vec<_> = name_values(w).objects.values().copied().collect();
        let mut out: Vec<String> = all
            .into_iter()
            .map(|o| {
                let g = match o.group {
                    Group::Function(f) => format!("fn {}", name(w, f)),
                    Group::Holder(Some(h)) => format!("holder {}", name(w, h)),
                    Group::Holder(None) => "none".to_owned(),
                };
                format!("{g} {} file={}", o.n, o.file.is_some())
            })
            .collect();
        out.sort();
        out
    }

    // Item 4: every `__impl` object a use built points to a file name
    // that is a string literal, so `write_location`'s `opt(false)` is
    // never written: `record_source_loc` (kiln_sema/src/builtins.rs)
    // returns before recording anything when `srcloc_string` of the file
    // fails, and the `__impl` object's `_M_file_name` is that pointer; a
    // deduced return type's refresh (`finish_source_locs`) keeps it.
    #[test]
    fn wp115_every_impl_object_has_its_file() {
        let got = with_walk(
            "struct S { std::source_location l = std::source_location::current(); };
constexpr S s{};
constexpr std::source_location g = std::source_location::current();
auto f() { return std::source_location::current(); }
void h(std::source_location = std::source_location::current());
void k() { h(); S t; (void)t; (void)f(); }
template <int N> constexpr std::source_location V = std::source_location::current();
constexpr auto v = V<1>;
",
            objects,
        );
        assert!(got.len() >= 6, "{got:?}");
        assert!(got.iter().all(|o| o.ends_with("file=true")), "{got:?}");
    }

    // Item 2: the holder of each kind of use outside a function.
    #[test]
    fn wp115_impl_objects_by_holder() {
        let got = with_walk(
            "struct S { std::source_location l = std::source_location::current(); };
constexpr S s2[2] = {S{}, S{}};
constexpr std::source_location g2[2] = {std::source_location::current(), std::source_location::current()};
template <int N> constexpr std::source_location V = std::source_location::current();
constexpr auto v = V<1>;
constexpr auto l = [x = std::source_location::current()] { return x; };
void k() { constexpr S t{}; (void)t; }
enum E : unsigned { X = std::source_location::current().line() };
",
            objects,
        );
        let want = [
            "fn k 0 file=true",
            "holder V 0 file=true",
            "holder current 0 file=true",
            "holder g2 0 file=true",
            "holder g2 1 file=true",
            // The lambda's variable `l`, and the field `S::l`'s own use.
            "holder l 0 file=true",
            "holder l 0 file=true",
            "holder s2 0 file=true",
            "holder s2 1 file=true",
            "none 0 file=true",
        ];
        assert_eq!(got, want);
    }

    // Item 5: two overloads one macro invocation makes share an extent,
    // so a `__func__` string, which both could hold, is its bytes; a
    // `__PRETTY_FUNCTION__` string, which only one holds, is that one's.
    #[test]
    fn wp115_overloads_of_one_macro_fall_back_to_the_bytes() {
        let got = with_walk(
            "void take(const char *);
#define TWO void f(int) { constexpr const char *p = __func__, *q = __PRETTY_FUNCTION__; take(p); take(q); } \\
  void f(long) { constexpr const char *p = __func__, *q = __PRETTY_FUNCTION__; take(p); take(q); }
TWO
",
            |w| {
                use slip_kiln::kiln::ast::{ExprId, RawId};
                use slip_kiln::kiln::ast::expr::ExprKind;
                let mut out = Vec::new();
                for i in 0..w.ast.exprs.len() {
                    let Some(e) = ExprId::from_raw(u32::try_from(i + 1).expect("fits")) else {
                        continue;
                    };
                    if w.ast.exprs.get(e).kind() != ExprKind::PredefinedExpr {
                        continue;
                    }
                    let Some(origin) = w.ast.exprs.predefined_name(e) else { continue };
                    let lit = w.ast.exprs.string_lit(origin);
                    let found = super::name_string(w, lit, origin).map(|n| match n {
                        super::NameString::Predefined(_, f) => format!("{f:?}"),
                        super::NameString::SourceLoc(_, f) => format!("loc {f:?}"),
                    });
                    let (_, bytes) = w.ast.strings.get(lit);
                    out.push((String::from_utf8_lossy(bytes).into_owned(), found.is_some()));
                }
                out
            },
        );
        assert_eq!(
            got,
            [
                ("f".to_owned(), false),
                ("void f(int)".to_owned(), true),
                ("f".to_owned(), false),
                ("void f(long)".to_owned(), true),
            ]
        );
    }
}
