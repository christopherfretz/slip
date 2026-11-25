//! Attributes and annotations (`spec/digest-kinds.md` §10.1, §10.5).
//!
//! An attribute node is written as `record AttrNode` (§2.1): its kind's
//! tag, its syntax's tag, one flag word (`flags-implicit`, then the `bit`
//! rows of its kind), then the `in` operand rows of its kind in table
//! order. `crate::tables` gives the operand rows as names and decisions
//! only, not their encodings, so each kind's rows are written here as
//! data ([`KINDS`]); a unit test checks that list against the generated
//! table row by row, and against the constants of Kiln's `attr.rs`.
//!
//! Every word of a node is accounted for: a flag bit, or a bit of `a` or
//! `b`, that no row of the node's kind names makes the unit
//! indeterminate (`Unaccounted("AttrNode/flags")`, `"AttrNode/a"`,
//! `"AttrNode/b"`), and so does a value outside what a row's note allows
//! (`Unaccounted("AttrNode/<row>")`).

use slip_kiln::kiln::ast::StringLitId;
use slip_kiln::kiln::ast::annot::Annotation;
use slip_kiln::kiln::ast::attr::{
    AttrKind, AttrNode, CLANG_SECTION_KINDS, LoopHint, ZERO_CALL_USED_REGS_KINDS,
};
use slip_kiln::kiln::ast::ids::RawId;
use slip_kiln::kiln::ast::sanitize::{NO_SANITIZE_SPELLINGS, SanitizerSet, no_sanitize_set};
use slip_kiln::kiln::ast::types::ParamAbi;
use slip_kiln::kiln::ast::{AnnId, AttrId, ConstValueId, DeclId, ExprId, ListId, QualType};
use slip_kiln::kiln::source::symbol::Symbol;

use crate::Cause;
use crate::stream::{FlagWord, Stream};
use crate::tables::{
    Decision, Row, attr_kind, attr_syntax, loop_hint_option, loop_hint_spelling, loop_hint_state,
    param_abi,
};
use crate::walk::Walk;

/// `flags` bit 1: a copy from an earlier declaration (`AttrNode` row
/// `flags-inherited`). Kiln has no constant for it: it documents the bit
/// on `AttrNode::flags` (`kiln_ast/src/attr.rs`) and sets it at one site
/// (`kiln_sema/src/decl.rs`, the inherited asm label).
const INHERITED: u8 = 2;

/// `DIAGNOSE_IF` flag bits 2, 4 and 5 (`AttrNode` rows
/// `DIAGNOSE_IF.flags-*`). Kiln's constants are private to its sema
/// (`kiln_sema/src/diagnose_if.rs`, `DIAGNOSE_IF_ERROR`,
/// `DIAGNOSE_IF_ARG_DEPENDENT`, `DIAGNOSE_IF_GROUP`).
const DIAGNOSE_IF_ERROR: u8 = 4;
const DIAGNOSE_IF_ARG_DEPENDENT: u8 = 16;
const DIAGNOSE_IF_GROUP: u8 = 32;

/// The `b` bits of a `NO_SANITIZE` node (`kiln_ast::sanitize`): the high
/// bits of the set, the spelling's index, and `coverage`.
const NO_SANITIZE_B_SET: u32 = 0x0fff_ffff;
const NO_SANITIZE_B_SPELLING: u32 = 0x7000_0000;
const NO_SANITIZE_B_COVERAGE: u32 = 0x8000_0000;

/// One of the two argument words of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    A,
    B,
}

/// What a per-kind row of `AttrNode` reads and how it is written
/// (`spec/digest-kinds.md` §10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Enc {
    /// `bit`: the flag bit `mask`.
    Bit(u8),
    /// An `out` row on the flag bit `mask`: accounted for, not written.
    OutBit(u8),
    /// An `out` row on the whole word.
    Out(Word),
    /// An `out` row on the bits `mask` of the word.
    OutBits(Word, u32),
    /// `ref`: the word is a raw `DeclId`.
    Ref(Word),
    /// `child`: the word is a raw `ExprId`.
    Child(Word),
    /// `child?`: a raw `ExprId`, 0 when absent.
    ChildOpt(Word),
    /// `child*`: a raw `ListId` of `ExprId`s; with `true`, 0 is the
    /// empty list.
    Children(Word, bool),
    /// `type`: a raw `QualType`.
    Type(Word),
    /// `type?`: a raw `QualType`, 0 when absent.
    TypeOpt(Word),
    /// `type?`, present exactly when the flag bit `mask` is set.
    TypeIfFlag(Word, u8),
    /// `string`: a raw `StringLitId`.
    String(Word),
    /// `string?`: a raw `StringLitId`, 0 when absent.
    StringOpt(Word),
    /// `uv`: the word.
    Uv(Word),
    /// `uv`: the word, which must lie in `lo..=hi`.
    UvIn(Word, u32, u32),
    /// `uv?`: the word, present when the flag bit `mask` is clear.
    UvUnlessFlag(Word, u8),
    /// `child?`: a raw `ExprId`, present when the flag bit `mask` is set.
    ChildIfFlag(Word, u8),
    /// `uv*`: a raw `ListId` of plain words.
    Uvs(Word),
    /// `sym`: a `Symbol`'s index.
    Sym(Word),
    /// `enum ParamAbi`.
    ParamAbi(Word),
    /// `record LoopHint`, packed (`LoopHint::pack`).
    LoopHint(Word),
    /// `record SanitizerSet`: `a` with the set bits of `b`
    /// (`NO_SANITIZE.a`).
    Sanitizers,
    /// `bool`: whether the bits `mask` of the word are set.
    BoolBits(Word, u32),
}

impl Enc {
    #[cfg(test)]
    /// The row's decision, as the table must say it.
    fn decision(self) -> Decision {
        match self {
            Enc::OutBit(_) | Enc::Out(_) | Enc::OutBits(..) => Decision::Out,
            Enc::Bit(_)
            | Enc::Ref(_)
            | Enc::Child(_)
            | Enc::ChildOpt(_)
            | Enc::Children(..)
            | Enc::Type(_)
            | Enc::TypeOpt(_)
            | Enc::TypeIfFlag(..)
            | Enc::String(_)
            | Enc::StringOpt(_)
            | Enc::Uv(_)
            | Enc::UvIn(..)
            | Enc::UvUnlessFlag(..)
            | Enc::ChildIfFlag(..)
            | Enc::Uvs(_)
            | Enc::Sym(_)
            | Enc::ParamAbi(_)
            | Enc::LoopHint(_)
            | Enc::Sanitizers
            | Enc::BoolBits(..) => Decision::In,
        }
    }

    /// The flag bits the row reads.
    fn flag_mask(self) -> u8 {
        match self {
            Enc::Bit(m) | Enc::OutBit(m) => m,
            Enc::TypeIfFlag(..)
            | Enc::UvUnlessFlag(..)
            | Enc::ChildIfFlag(..)
            | Enc::Out(_)
            | Enc::OutBits(..)
            | Enc::Ref(_)
            | Enc::Child(_)
            | Enc::ChildOpt(_)
            | Enc::Children(..)
            | Enc::Type(_)
            | Enc::TypeOpt(_)
            | Enc::String(_)
            | Enc::StringOpt(_)
            | Enc::Uv(_)
            | Enc::UvIn(..)
            | Enc::Uvs(_)
            | Enc::Sym(_)
            | Enc::ParamAbi(_)
            | Enc::LoopHint(_)
            | Enc::Sanitizers
            | Enc::BoolBits(..) => 0,
        }
    }

    /// The bits of `a` and of `b` the row reads.
    fn word_masks(self) -> (u32, u32) {
        let whole = |w: Word| match w {
            Word::A => (u32::MAX, 0),
            Word::B => (0, u32::MAX),
        };
        match self {
            Enc::Bit(_) | Enc::OutBit(_) => (0, 0),
            Enc::OutBits(w, m) | Enc::BoolBits(w, m) => match w {
                Word::A => (m, 0),
                Word::B => (0, m),
            },
            // `a` and the set bits of `b`; those bits are also `b-set`'s.
            Enc::Sanitizers => (u32::MAX, NO_SANITIZE_B_SET),
            Enc::Out(w)
            | Enc::Ref(w)
            | Enc::Child(w)
            | Enc::ChildOpt(w)
            | Enc::Children(w, _)
            | Enc::Type(w)
            | Enc::TypeOpt(w)
            | Enc::TypeIfFlag(w, _)
            | Enc::String(w)
            | Enc::StringOpt(w)
            | Enc::Uv(w)
            | Enc::UvIn(w, ..)
            | Enc::UvUnlessFlag(w, _)
            | Enc::ChildIfFlag(w, _)
            | Enc::Uvs(w)
            | Enc::Sym(w)
            | Enc::ParamAbi(w)
            | Enc::LoopHint(w) => whole(w),
        }
    }
}

/// A per-kind row of `AttrNode`: its test name (`AttrNode/<row>`, also
/// the cause of a value it does not account for) and its encoding.
#[derive(Clone, Copy, Debug)]
struct Operand {
    name: &'static str,
    enc: Enc,
}

/// A row of `AttrKind` and the per-kind rows of `AttrNode` for it, in
/// table order.
#[derive(Clone, Copy, Debug)]
struct KindRow {
    kind: AttrKind,
    row: Row,
    /// `AttrKind/<row>`: the test name and the cause of an `unsupported`
    /// row.
    name: &'static str,
    ops: &'static [Operand],
}

const fn op(name: &'static str, enc: Enc) -> Operand {
    Operand { name, enc }
}

const fn k(kind: AttrKind, row: Row, name: &'static str, ops: &'static [Operand]) -> KindRow {
    KindRow { kind, row, name, ops }
}

/// `child?` `a`, `child?` `b`: the shape of the message attributes.
macro_rules! two_children {
    ($kind:literal) => {
        &[
            op(concat!("AttrNode/", $kind, ".a"), Enc::ChildOpt(Word::A)),
            op(concat!("AttrNode/", $kind, ".b"), Enc::ChildOpt(Word::B)),
        ]
    };
}

/// The rows of `AttrKind`, in table order, each with its per-kind rows of
/// `AttrNode` in table order (`spec/digest-kinds.md` §10.1).
#[rustfmt::skip]
const KINDS: &[KindRow] = {
    use Enc::{
        Bit, BoolBits, Child, ChildIfFlag, ChildOpt, Children, Out, OutBit, OutBits,
        Sanitizers, String, StringOpt, Sym, Type, TypeIfFlag, TypeOpt, Uv, UvIn,
        UvUnlessFlag, Uvs,
    };
    use Word::{A, B};
    const PRIO: u8 = AttrNode::PRIORITY_DEPENDENT;
    const AV_DEP: u8 = AttrNode::ALIGN_VALUE_DEPENDENT;
    const GSL: u8 = AttrNode::GSL_HAS_TYPE;
    const ZCUR: u32 = ZERO_CALL_USED_REGS_KINDS.len() as u32 - 1;
    const PCS: u32 = CLANG_SECTION_KINDS.len() as u32 - 1;
    &[
        k(AttrKind::ALIGNED, attr_kind::ALIGNED, "AttrKind/ALIGNED", &[
            op("AttrNode/ALIGNED.a", ChildOpt(A)),
            op("AttrNode/ALIGNED.b", TypeOpt(B)),
        ]),
        k(AttrKind::ASSUME, attr_kind::ASSUME, "AttrKind/ASSUME", &[
            op("AttrNode/ASSUME.flags-ignored", Bit(AttrNode::ASSUME_IGNORED)),
            op("AttrNode/ASSUME.a", Child(A)),
            op("AttrNode/ASSUME.b", Out(B)),
        ]),
        k(AttrKind::CARRIES_DEPENDENCY, attr_kind::CARRIES_DEPENDENCY, "AttrKind/CARRIES_DEPENDENCY", &[]),
        k(AttrKind::DEPRECATED, attr_kind::DEPRECATED, "AttrKind/DEPRECATED", two_children!("DEPRECATED")),
        k(AttrKind::FALLTHROUGH, attr_kind::FALLTHROUGH, "AttrKind/FALLTHROUGH", &[
            op("AttrNode/FALLTHROUGH.flags-scoped", OutBit(AttrNode::FALLTHROUGH_SCOPED)),
        ]),
        k(AttrKind::INDETERMINATE, attr_kind::INDETERMINATE, "AttrKind/INDETERMINATE", &[]),
        k(AttrKind::LIKELY, attr_kind::LIKELY, "AttrKind/LIKELY", &[]),
        k(AttrKind::UNLIKELY, attr_kind::UNLIKELY, "AttrKind/UNLIKELY", &[]),
        k(AttrKind::MAYBE_UNUSED, attr_kind::MAYBE_UNUSED, "AttrKind/MAYBE_UNUSED", &[]),
        k(AttrKind::NODISCARD, attr_kind::NODISCARD, "AttrKind/NODISCARD", &[
            op("AttrNode/NODISCARD.flags-clang-scoped", OutBit(AttrNode::CLANG_SCOPED)),
            op("AttrNode/NODISCARD.a", ChildOpt(A)),
            op("AttrNode/NODISCARD.b", ChildOpt(B)),
        ]),
        k(AttrKind::NORETURN, attr_kind::NORETURN, "AttrKind/NORETURN", &[]),
        k(AttrKind::NO_UNIQUE_ADDRESS, attr_kind::NO_UNIQUE_ADDRESS, "AttrKind/NO_UNIQUE_ADDRESS", &[]),
        k(AttrKind::ANNOTATION, attr_kind::ANNOTATION, "AttrKind/ANNOTATION", &[]),
        k(AttrKind::ALWAYS_INLINE, attr_kind::ALWAYS_INLINE, "AttrKind/ALWAYS_INLINE", &[]),
        k(AttrKind::NOINLINE, attr_kind::NOINLINE, "AttrKind/NOINLINE", &[]),
        k(AttrKind::PACKED, attr_kind::PACKED, "AttrKind/PACKED", &[]),
        k(AttrKind::USED, attr_kind::USED, "AttrKind/USED", &[]),
        k(AttrKind::UNUSED, attr_kind::UNUSED, "AttrKind/UNUSED", &[]),
        k(AttrKind::WEAK, attr_kind::WEAK, "AttrKind/WEAK", &[]),
        k(AttrKind::VISIBILITY, attr_kind::VISIBILITY, "AttrKind/VISIBILITY", &[
            op("AttrNode/VISIBILITY.a", UvIn(A, 0, 2)),
            op("AttrNode/VISIBILITY.b", Out(B)),
        ]),
        k(AttrKind::SECTION, attr_kind::SECTION, "AttrKind/SECTION", &[
            op("AttrNode/SECTION.a", StringOpt(A)),
        ]),
        k(AttrKind::FORMAT, attr_kind::FORMAT, "AttrKind/FORMAT", &[
            op("AttrNode/FORMAT.a", Uv(A)),
            op("AttrNode/FORMAT.b", Uv(B)),
        ]),
        k(AttrKind::CONST, attr_kind::CONST, "AttrKind/CONST", &[]),
        k(AttrKind::PURE, attr_kind::PURE, "AttrKind/PURE", &[]),
        k(AttrKind::COLD, attr_kind::COLD, "AttrKind/COLD", &[]),
        k(AttrKind::NOTHROW, attr_kind::NOTHROW, "AttrKind/NOTHROW", &[]),
        k(AttrKind::ENABLE_IF, attr_kind::ENABLE_IF, "AttrKind/ENABLE_IF", two_children!("ENABLE_IF")),
        k(AttrKind::DIAGNOSE_IF, attr_kind::DIAGNOSE_IF, "AttrKind/DIAGNOSE_IF", &[
            op("AttrNode/DIAGNOSE_IF.flags-error", Bit(DIAGNOSE_IF_ERROR)),
            op("AttrNode/DIAGNOSE_IF.flags-arg-dependent", Bit(DIAGNOSE_IF_ARG_DEPENDENT)),
            op("AttrNode/DIAGNOSE_IF.flags-group", Bit(DIAGNOSE_IF_GROUP)),
            op("AttrNode/DIAGNOSE_IF.a", ChildOpt(A)),
            op("AttrNode/DIAGNOSE_IF.b", ChildOpt(B)),
        ]),
        k(AttrKind::TRIVIAL_ABI, attr_kind::TRIVIAL_ABI, "AttrKind/TRIVIAL_ABI", &[]),
        k(AttrKind::PREFERRED_NAME, attr_kind::PREFERRED_NAME, "AttrKind/PREFERRED_NAME", &[
            op("AttrNode/PREFERRED_NAME.b", Type(B)),
        ]),
        k(AttrKind::EXCLUDE_FROM_EXPLICIT_INSTANTIATION, attr_kind::EXCLUDE_FROM_EXPLICIT_INSTANTIATION, "AttrKind/EXCLUDE_FROM_EXPLICIT_INSTANTIATION", &[]),
        k(AttrKind::ABI_TAG, attr_kind::ABI_TAG, "AttrKind/ABI_TAG", &[
            op("AttrNode/ABI_TAG.a", Sym(A)),
        ]),
        k(AttrKind::INTERNAL_LINKAGE, attr_kind::INTERNAL_LINKAGE, "AttrKind/INTERNAL_LINKAGE", &[]),
        k(AttrKind::LIFETIMEBOUND, attr_kind::LIFETIMEBOUND, "AttrKind/LIFETIMEBOUND", &[
            op("AttrNode/LIFETIMEBOUND.flags-args", OutBit(AttrNode::LIFETIMEBOUND_ARGS)),
            op("AttrNode/LIFETIMEBOUND.a", UvIn(A, 0, 1)),
        ]),
        k(AttrKind::NO_SANITIZE, attr_kind::NO_SANITIZE, "AttrKind/NO_SANITIZE", &[
            op("AttrNode/NO_SANITIZE.a", Sanitizers),
            op("AttrNode/NO_SANITIZE.b-set", OutBits(B, NO_SANITIZE_B_SET)),
            op("AttrNode/NO_SANITIZE.b-spelling", OutBits(B, NO_SANITIZE_B_SPELLING)),
            op("AttrNode/NO_SANITIZE.b-coverage", BoolBits(B, NO_SANITIZE_B_COVERAGE)),
        ]),
        k(AttrKind::MODE, attr_kind::MODE, "AttrKind/MODE", &[
            op("AttrNode/MODE.flags-deferred", Bit(AttrNode::MODE_DEFERRED)),
            op("AttrNode/MODE.a", Uv(A)),
            op("AttrNode/MODE.b", Out(B)),
        ]),
        k(AttrKind::VECTOR_SIZE, attr_kind::VECTOR_SIZE, "AttrKind/VECTOR_SIZE", &[]),
        k(AttrKind::ASM_LABEL, attr_kind::ASM_LABEL, "AttrKind/ASM_LABEL", &[
            op("AttrNode/ASM_LABEL.a", String(A)),
        ]),
        k(AttrKind::BUILTIN, attr_kind::BUILTIN, "AttrKind/BUILTIN", &[
            op("AttrNode/BUILTIN.a", Out(A)),
        ]),
        k(AttrKind::EXT_VECTOR_TYPE, attr_kind::EXT_VECTOR_TYPE, "AttrKind/EXT_VECTOR_TYPE", &[]),
        k(AttrKind::MALLOC, attr_kind::MALLOC, "AttrKind/MALLOC", &[]),
        k(AttrKind::ALLOC_SIZE, attr_kind::ALLOC_SIZE, "AttrKind/ALLOC_SIZE", two_children!("ALLOC_SIZE")),
        k(AttrKind::ALLOC_ALIGN, attr_kind::ALLOC_ALIGN, "AttrKind/ALLOC_ALIGN", two_children!("ALLOC_ALIGN")),
        k(AttrKind::USING_IF_EXISTS, attr_kind::USING_IF_EXISTS, "AttrKind/USING_IF_EXISTS", &[]),
        k(AttrKind::HOT, attr_kind::HOT, "AttrKind/HOT", &[]),
        k(AttrKind::FLAG_ENUM, attr_kind::FLAG_ENUM, "AttrKind/FLAG_ENUM", &[]),
        k(AttrKind::ENUM_EXTENSIBILITY, attr_kind::ENUM_EXTENSIBILITY, "AttrKind/ENUM_EXTENSIBILITY", &[
            op("AttrNode/ENUM_EXTENSIBILITY.a", UvIn(A, 0, 1)),
        ]),
        k(AttrKind::CORO_ONLY_DESTROY_WHEN_COMPLETE, attr_kind::CORO_ONLY_DESTROY_WHEN_COMPLETE, "AttrKind/CORO_ONLY_DESTROY_WHEN_COMPLETE", &[]),
        k(AttrKind::OPTNONE, attr_kind::OPTNONE, "AttrKind/OPTNONE", &[]),
        k(AttrKind::MINSIZE, attr_kind::MINSIZE, "AttrKind/MINSIZE", &[]),
        k(AttrKind::NAKED, attr_kind::NAKED, "AttrKind/NAKED", &[]),
        k(AttrKind::MAY_ALIAS, attr_kind::MAY_ALIAS, "AttrKind/MAY_ALIAS", &[]),
        k(AttrKind::NONNULL, attr_kind::NONNULL, "AttrKind/NONNULL", &[
            op("AttrNode/NONNULL.a", Children(A, false)),
        ]),
        k(AttrKind::RETURNS_NONNULL, attr_kind::RETURNS_NONNULL, "AttrKind/RETURNS_NONNULL", &[]),
        k(AttrKind::INIT_PRIORITY, attr_kind::INIT_PRIORITY, "AttrKind/INIT_PRIORITY", &[
            op("AttrNode/INIT_PRIORITY.a", Uv(A)),
        ]),
        k(AttrKind::LOOP_HINT, attr_kind::LOOP_HINT, "AttrKind/LOOP_HINT", &[
            op("AttrNode/LOOP_HINT.a", ChildOpt(A)),
            op("AttrNode/LOOP_HINT.b", Enc::LoopHint(B)),
        ]),
        k(AttrKind::UNINITIALIZED, attr_kind::UNINITIALIZED, "AttrKind/UNINITIALIZED", &[]),
        k(AttrKind::TYPE_VISIBILITY, attr_kind::TYPE_VISIBILITY, "AttrKind/TYPE_VISIBILITY", &[
            op("AttrNode/TYPE_VISIBILITY.a", UvIn(A, 0, 2)),
        ]),
        k(AttrKind::TYPE_NULLABLE, attr_kind::TYPE_NULLABLE, "AttrKind/TYPE_NULLABLE", &[]),
        k(AttrKind::NEON_VECTOR_TYPE, attr_kind::NEON_VECTOR_TYPE, "AttrKind/NEON_VECTOR_TYPE", &[]),
        k(AttrKind::NEON_POLYVECTOR_TYPE, attr_kind::NEON_POLYVECTOR_TYPE, "AttrKind/NEON_POLYVECTOR_TYPE", &[]),
        k(AttrKind::TARGET, attr_kind::TARGET, "AttrKind/TARGET", &[
            op("AttrNode/TARGET.a", StringOpt(A)),
        ]),
        k(AttrKind::GNU_INLINE, attr_kind::GNU_INLINE, "AttrKind/GNU_INLINE", &[]),
        k(AttrKind::NODEBUG, attr_kind::NODEBUG, "AttrKind/NODEBUG", &[]),
        k(AttrKind::MIN_VECTOR_WIDTH, attr_kind::MIN_VECTOR_WIDTH, "AttrKind/MIN_VECTOR_WIDTH", &[
            op("AttrNode/MIN_VECTOR_WIDTH.a", Uv(A)),
        ]),
        k(AttrKind::WEAKREF, attr_kind::WEAKREF, "AttrKind/WEAKREF", &[
            op("AttrNode/WEAKREF.a", StringOpt(A)),
        ]),
        k(AttrKind::ALIAS, attr_kind::ALIAS, "AttrKind/ALIAS", &[
            op("AttrNode/ALIAS.a", String(A)),
            op("AttrNode/ALIAS.b", Out(B)),
        ]),
        k(AttrKind::ANALYZER_NORETURN, attr_kind::ANALYZER_NORETURN, "AttrKind/ANALYZER_NORETURN", &[]),
        k(AttrKind::DISABLE_TAIL_CALLS, attr_kind::DISABLE_TAIL_CALLS, "AttrKind/DISABLE_TAIL_CALLS", &[]),
        k(AttrKind::IFUNC, attr_kind::IFUNC, "AttrKind/IFUNC", &[
            op("AttrNode/IFUNC.a", String(A)),
            op("AttrNode/IFUNC.b", Out(B)),
        ]),
        k(AttrKind::ARTIFICIAL, attr_kind::ARTIFICIAL, "AttrKind/ARTIFICIAL", &[]),
        k(AttrKind::CONSTRUCTOR, attr_kind::CONSTRUCTOR, "AttrKind/CONSTRUCTOR", &[
            op("AttrNode/CONSTRUCTOR.flags-priority-dependent", Bit(PRIO)),
            op("AttrNode/CONSTRUCTOR.a-priority", UvUnlessFlag(A, PRIO)),
            op("AttrNode/CONSTRUCTOR.a-expression", ChildIfFlag(A, PRIO)),
        ]),
        k(AttrKind::DESTRUCTOR, attr_kind::DESTRUCTOR, "AttrKind/DESTRUCTOR", &[
            op("AttrNode/DESTRUCTOR.flags-priority-dependent", Bit(PRIO)),
            op("AttrNode/DESTRUCTOR.a-priority", UvUnlessFlag(A, PRIO)),
            op("AttrNode/DESTRUCTOR.a-expression", ChildIfFlag(A, PRIO)),
        ]),
        k(AttrKind::CLEANUP, attr_kind::CLEANUP, "AttrKind/CLEANUP", &[
            op("AttrNode/CLEANUP.a", Enc::Ref(A)),
            op("AttrNode/CLEANUP.b", Out(B)),
        ]),
        k(AttrKind::COMMON, attr_kind::COMMON, "AttrKind/COMMON", &[]),
        k(AttrKind::TLS_MODEL, attr_kind::TLS_MODEL, "AttrKind/TLS_MODEL", &[
            op("AttrNode/TLS_MODEL.a", UvIn(A, 1, 4)),
        ]),
        k(AttrKind::RETURNS_TWICE, attr_kind::RETURNS_TWICE, "AttrKind/RETURNS_TWICE", &[]),
        k(AttrKind::PASS_OBJECT_SIZE, attr_kind::PASS_OBJECT_SIZE, "AttrKind/PASS_OBJECT_SIZE", &[
            op("AttrNode/PASS_OBJECT_SIZE.a", UvIn(A, 0, 3)),
            op("AttrNode/PASS_OBJECT_SIZE.b", UvIn(B, 0, 1)),
        ]),
        k(AttrKind::OVERLOADABLE, attr_kind::OVERLOADABLE, "AttrKind/OVERLOADABLE", &[]),
        k(AttrKind::MUSTTAIL, attr_kind::MUSTTAIL, "AttrKind/MUSTTAIL", &[]),
        k(AttrKind::TRANSPARENT_UNION, attr_kind::TRANSPARENT_UNION, "AttrKind/TRANSPARENT_UNION", &[]),
        k(AttrKind::SELECTANY, attr_kind::SELECTANY, "AttrKind/SELECTANY", &[]),
        k(AttrKind::MS_STRUCT, attr_kind::MS_STRUCT, "AttrKind/MS_STRUCT", &[]),
        k(AttrKind::GCC_STRUCT, attr_kind::GCC_STRUCT, "AttrKind/GCC_STRUCT", &[]),
        k(AttrKind::RETAIN, attr_kind::RETAIN, "AttrKind/RETAIN", &[]),
        k(AttrKind::NO_DESTROY, attr_kind::NO_DESTROY, "AttrKind/NO_DESTROY", &[]),
        k(AttrKind::ALWAYS_DESTROY, attr_kind::ALWAYS_DESTROY, "AttrKind/ALWAYS_DESTROY", &[]),
        k(AttrKind::CALLING_CONV, attr_kind::CALLING_CONV, "AttrKind/CALLING_CONV", &[]),
        k(AttrKind::NO_CALLER_SAVED_REGS, attr_kind::NO_CALLER_SAVED_REGS, "AttrKind/NO_CALLER_SAVED_REGS", &[]),
        k(AttrKind::REGPARM, attr_kind::REGPARM, "AttrKind/REGPARM", &[]),
        k(AttrKind::NO_BUILTIN, attr_kind::NO_BUILTIN, "AttrKind/NO_BUILTIN", &[
            op("AttrNode/NO_BUILTIN.a", Sym(A)),
        ]),
        k(AttrKind::ARM_SVE_VECTOR_BITS, attr_kind::ARM_SVE_VECTOR_BITS, "AttrKind/ARM_SVE_VECTOR_BITS", &[]),
        k(AttrKind::BUILTIN_ALIAS, attr_kind::BUILTIN_ALIAS, "AttrKind/BUILTIN_ALIAS", &[
            op("AttrNode/BUILTIN_ALIAS.a", Sym(A)),
            op("AttrNode/BUILTIN_ALIAS.b", Out(B)),
        ]),
        k(AttrKind::TARGET_VERSION, attr_kind::TARGET_VERSION, "AttrKind/TARGET_VERSION", &[
            op("AttrNode/TARGET_VERSION.a", String(A)),
            op("AttrNode/TARGET_VERSION.b", Out(B)),
        ]),
        k(AttrKind::TARGET_CLONES, attr_kind::TARGET_CLONES, "AttrKind/TARGET_CLONES", &[
            op("AttrNode/TARGET_CLONES.a", Sym(A)),
            op("AttrNode/TARGET_CLONES.b", UvIn(B, 0, 1)),
        ]),
        k(AttrKind::CPU_DISPATCH, attr_kind::CPU_DISPATCH, "AttrKind/CPU_DISPATCH", &[]),
        k(AttrKind::CPU_SPECIFIC, attr_kind::CPU_SPECIFIC, "AttrKind/CPU_SPECIFIC", &[]),
        k(AttrKind::NO_STACK_PROTECTOR, attr_kind::NO_STACK_PROTECTOR, "AttrKind/NO_STACK_PROTECTOR", &[]),
        k(AttrKind::PATCHABLE_FUNCTION_ENTRY, attr_kind::PATCHABLE_FUNCTION_ENTRY, "AttrKind/PATCHABLE_FUNCTION_ENTRY", &[
            op("AttrNode/PATCHABLE_FUNCTION_ENTRY.a", Uv(A)),
            op("AttrNode/PATCHABLE_FUNCTION_ENTRY.b", Uv(B)),
        ]),
        k(AttrKind::ZERO_CALL_USED_REGS, attr_kind::ZERO_CALL_USED_REGS, "AttrKind/ZERO_CALL_USED_REGS", &[
            op("AttrNode/ZERO_CALL_USED_REGS.a", UvIn(A, 0, ZCUR)),
        ]),
        k(AttrKind::SPECULATIVE_LOAD_HARDENING, attr_kind::SPECULATIVE_LOAD_HARDENING, "AttrKind/SPECULATIVE_LOAD_HARDENING", &[]),
        k(AttrKind::NO_SPECULATIVE_LOAD_HARDENING, attr_kind::NO_SPECULATIVE_LOAD_HARDENING, "AttrKind/NO_SPECULATIVE_LOAD_HARDENING", &[]),
        k(AttrKind::NO_SPLIT_STACK, attr_kind::NO_SPLIT_STACK, "AttrKind/NO_SPLIT_STACK", &[]),
        k(AttrKind::NO_INSTRUMENT_FUNCTION, attr_kind::NO_INSTRUMENT_FUNCTION, "AttrKind/NO_INSTRUMENT_FUNCTION", &[]),
        k(AttrKind::NO_PROFILE_INSTRUMENT_FUNCTION, attr_kind::NO_PROFILE_INSTRUMENT_FUNCTION, "AttrKind/NO_PROFILE_INSTRUMENT_FUNCTION", &[]),
        k(AttrKind::XRAY_INSTRUMENT, attr_kind::XRAY_INSTRUMENT, "AttrKind/XRAY_INSTRUMENT", &[
            op("AttrNode/XRAY_INSTRUMENT.a", UvIn(A, 0, 1)),
        ]),
        k(AttrKind::XRAY_LOG_ARGS, attr_kind::XRAY_LOG_ARGS, "AttrKind/XRAY_LOG_ARGS", &[
            op("AttrNode/XRAY_LOG_ARGS.a", Uv(A)),
        ]),
        k(AttrKind::DISABLE_SANITIZER_INSTRUMENTATION, attr_kind::DISABLE_SANITIZER_INSTRUMENTATION, "AttrKind/DISABLE_SANITIZER_INSTRUMENTATION", &[]),
        k(AttrKind::NOCF_CHECK, attr_kind::NOCF_CHECK, "AttrKind/NOCF_CHECK", &[]),
        k(AttrKind::FUNCTION_RETURN, attr_kind::FUNCTION_RETURN, "AttrKind/FUNCTION_RETURN", &[
            op("AttrNode/FUNCTION_RETURN.a", UvIn(A, 0, 1)),
        ]),
        k(AttrKind::CFI_CANONICAL_JUMP_TABLE, attr_kind::CFI_CANONICAL_JUMP_TABLE, "AttrKind/CFI_CANONICAL_JUMP_TABLE", &[]),
        k(AttrKind::FORCE_ALIGN_ARG_POINTER, attr_kind::FORCE_ALIGN_ARG_POINTER, "AttrKind/FORCE_ALIGN_ARG_POINTER", &[]),
        k(AttrKind::FLATTEN, attr_kind::FLATTEN, "AttrKind/FLATTEN", &[]),
        k(AttrKind::LEAF, attr_kind::LEAF, "AttrKind/LEAF", &[]),
        k(AttrKind::NOMERGE, attr_kind::NOMERGE, "AttrKind/NOMERGE", &[]),
        k(AttrKind::NOT_TAIL_CALLED, attr_kind::NOT_TAIL_CALLED, "AttrKind/NOT_TAIL_CALLED", &[]),
        k(AttrKind::NODUPLICATE, attr_kind::NODUPLICATE, "AttrKind/NODUPLICATE", &[]),
        k(AttrKind::CONVERGENT, attr_kind::CONVERGENT, "AttrKind/CONVERGENT", &[]),
        k(AttrKind::NOUWTABLE, attr_kind::NOUWTABLE, "AttrKind/NOUWTABLE", &[]),
        k(AttrKind::ASSUME_ALIGNED, attr_kind::ASSUME_ALIGNED, "AttrKind/ASSUME_ALIGNED", &[
            op("AttrNode/ASSUME_ALIGNED.a", Uv(A)),
            op("AttrNode/ASSUME_ALIGNED.b", Uv(B)),
        ]),
        k(AttrKind::ALIGN_VALUE, attr_kind::ALIGN_VALUE, "AttrKind/ALIGN_VALUE", &[
            op("AttrNode/ALIGN_VALUE.flags-value-dependent", Bit(AV_DEP)),
            op("AttrNode/ALIGN_VALUE.a-alignment", UvUnlessFlag(A, AV_DEP)),
            op("AttrNode/ALIGN_VALUE.a-expression", ChildIfFlag(A, AV_DEP)),
        ]),
        k(AttrKind::CALLBACK, attr_kind::CALLBACK, "AttrKind/CALLBACK", &[
            op("AttrNode/CALLBACK.a", Uvs(A)),
        ]),
        k(AttrKind::NOESCAPE, attr_kind::NOESCAPE, "AttrKind/NOESCAPE", &[]),
        k(AttrKind::MAYBE_UNDEF, attr_kind::MAYBE_UNDEF, "AttrKind/MAYBE_UNDEF", &[]),
        k(AttrKind::CODE_ALIGN, attr_kind::CODE_ALIGN, "AttrKind/CODE_ALIGN", &[
            op("AttrNode/CODE_ALIGN.a", Uv(A)),
        ]),
        k(AttrKind::LOADER_UNINITIALIZED, attr_kind::LOADER_UNINITIALIZED, "AttrKind/LOADER_UNINITIALIZED", &[]),
        k(AttrKind::LTO_VISIBILITY_PUBLIC, attr_kind::LTO_VISIBILITY_PUBLIC, "AttrKind/LTO_VISIBILITY_PUBLIC", &[]),
        k(AttrKind::OMP_ASSUME, attr_kind::OMP_ASSUME, "AttrKind/OMP_ASSUME", &[
            op("AttrNode/OMP_ASSUME.a", StringOpt(A)),
        ]),
        k(AttrKind::STANDALONE_DEBUG, attr_kind::STANDALONE_DEBUG, "AttrKind/STANDALONE_DEBUG", &[]),
        k(AttrKind::CORO_AWAIT_ELIDABLE, attr_kind::CORO_AWAIT_ELIDABLE, "AttrKind/CORO_AWAIT_ELIDABLE", &[]),
        k(AttrKind::CORO_AWAIT_ELIDABLE_ARGUMENT, attr_kind::CORO_AWAIT_ELIDABLE_ARGUMENT, "AttrKind/CORO_AWAIT_ELIDABLE_ARGUMENT", &[]),
        k(AttrKind::GSL_OWNER, attr_kind::GSL_OWNER, "AttrKind/GSL_OWNER", &[
            op("AttrNode/GSL_OWNER.flags-has-type", Bit(GSL)),
            op("AttrNode/GSL_OWNER.b", TypeIfFlag(B, GSL)),
        ]),
        k(AttrKind::GSL_POINTER, attr_kind::GSL_POINTER, "AttrKind/GSL_POINTER", &[
            op("AttrNode/GSL_POINTER.flags-has-type", Bit(GSL)),
            op("AttrNode/GSL_POINTER.b", TypeIfFlag(B, GSL)),
        ]),
        k(AttrKind::UNAVAILABLE, attr_kind::UNAVAILABLE, "AttrKind/UNAVAILABLE", two_children!("UNAVAILABLE")),
        k(AttrKind::REQUIRE_CONSTANT_INIT, attr_kind::REQUIRE_CONSTANT_INIT, "AttrKind/REQUIRE_CONSTANT_INIT", &[]),
        k(AttrKind::ANNOTATE, attr_kind::ANNOTATE, "AttrKind/ANNOTATE", &[
            op("AttrNode/ANNOTATE.a", ChildOpt(A)),
            op("AttrNode/ANNOTATE.b", Children(B, true)),
        ]),
        k(AttrKind::SME_TYPE, attr_kind::SME_TYPE, "AttrKind/SME_TYPE", &[]),
        k(AttrKind::ARM_LOCALLY_STREAMING, attr_kind::ARM_LOCALLY_STREAMING, "AttrKind/ARM_LOCALLY_STREAMING", &[
            op("AttrNode/ARM_LOCALLY_STREAMING.b", Out(B)),
        ]),
        k(AttrKind::ARM_NEW, attr_kind::ARM_NEW, "AttrKind/ARM_NEW", &[
            op("AttrNode/ARM_NEW.a", UvIn(A, 0, 3)),
            op("AttrNode/ARM_NEW.b", Out(B)),
        ]),
        k(AttrKind::NONSTRING, attr_kind::NONSTRING, "AttrKind/NONSTRING", &[]),
        k(AttrKind::NOCOMMON, attr_kind::NOCOMMON, "AttrKind/NOCOMMON", &[]),
        k(AttrKind::PRAGMA_CLANG_SECTION, attr_kind::PRAGMA_CLANG_SECTION, "AttrKind/PRAGMA_CLANG_SECTION", &[
            op("AttrNode/PRAGMA_CLANG_SECTION.a", StringOpt(A)),
            op("AttrNode/PRAGMA_CLANG_SECTION.b", UvIn(B, 0, PCS)),
        ]),
        k(AttrKind::CODE_MODEL, attr_kind::CODE_MODEL, "AttrKind/CODE_MODEL", &[
            op("AttrNode/CODE_MODEL.a", UvIn(A, 1, 2)),
        ]),
        k(AttrKind::X86_INTERRUPT, attr_kind::X86_INTERRUPT, "AttrKind/X86_INTERRUPT", &[]),
        k(AttrKind::WEAK_IMPORT, attr_kind::WEAK_IMPORT, "AttrKind/WEAK_IMPORT", &[]),
        k(AttrKind::SWIFT_PARAM_ABI, attr_kind::SWIFT_PARAM_ABI, "AttrKind/SWIFT_PARAM_ABI", &[
            op("AttrNode/SWIFT_PARAM_ABI.a", Enc::ParamAbi(A)),
        ]),
        k(AttrKind::ADDRESS_SPACE, attr_kind::ADDRESS_SPACE, "AttrKind/ADDRESS_SPACE", &[]),
    ]
};

/// The flag bits every kind accounts for: `flags-implicit` (`in`),
/// `flags-inherited`, `flags-gnu-scoped` and `flags-checked` (`out`).
const COMMON_FLAGS: u8 = AttrNode::IMPLICIT | INHERITED | AttrNode::GNU_SCOPED | AttrNode::CHECKED;

/// The row of `kind`, if the table has one.
fn kind_row(kind: AttrKind) -> Option<&'static KindRow> {
    KINDS.iter().find(|r| r.kind == kind)
}

/// The attribute kinds Kiln copies onto a function from the pattern's
/// definition when it instantiates the definition: `weak` from an
/// out-of-line member definition (`kiln_sema/src/templ/class.rs:1603-1615`),
/// `#pragma clang section` from any definition (`clone_clang_sections`,
/// `kiln_sema/src/section.rs:204`, called at `templ/class.rs:2313` and
/// `templ/funcs.rs:365`). Kiln does not mark the copies.
const COPIED_AT_DEFINITION: [AttrKind; 2] = [AttrKind::WEAK, AttrKind::PRAGMA_CLANG_SECTION];

/// Which attribute nodes of declaration `x` of entity `e` are
/// `definition`'s rather than the own unit's (§11.9), for an entity of
/// the definition class: none on a declaration written in source; on a
/// declaration made by instantiation (`templ::made_by_instantiation`),
/// every node for a class (Kiln copies the attributes and annotations of
/// the pattern's definition when it instantiates the class's definition,
/// `Substituter::class_attrs`, `kiln_sema/src/templ/subst/anon.rs:140`,
/// from `templ/class.rs:2530`), the kinds of [`COPIED_AT_DEFINITION`]
/// for a function, none otherwise. `None`: the entity is not of the
/// class, or is of a kind none of whose nodes move.
fn moved_kinds(w: &Walk<'_>, e: DeclId) -> Option<Moved> {
    let ast = w.ast;
    if !crate::demand::definition_class(w, e) {
        return None;
    }
    if ast.decls.record(e).is_some() {
        Some(Moved::All)
    } else if ast.decls.function(e).is_some() {
        Some(Moved::Copied)
    } else {
        None
    }
}

/// What [`moved_kinds`] moves of a declaration made by instantiation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Moved {
    /// Every attribute node and annotation (a class).
    All,
    /// The nodes of the kinds of [`COPIED_AT_DEFINITION`] (a function).
    Copied,
}

impl Moved {
    fn node(self, n: &AttrNode) -> bool {
        match self {
            Moved::All => true,
            Moved::Copied => COPIED_AT_DEFINITION.contains(&n.kind),
        }
    }
}

/// The attribute nodes (and, for a class, the annotations) of entity
/// `e` that [`moved_kinds`] moves, for the resolved unit `definition`
/// (§11.9): over the declarations made by instantiation, in chain order
/// (first to last), each one's moved nodes in stored order without the
/// `inherited` copies, as a list; the non-empty lists, preceded by their
/// count, as `attrs` writes them (§10.1); then, for a class, the
/// annotations of those declarations as one list.
pub(crate) fn definition_attrs(w: &mut Walk<'_>, s: &mut Stream, e: DeclId) {
    let moved = moved_kinds(w, e);
    let mut chain = w.redecls(e);
    chain.reverse();
    let made: Vec<DeclId> =
        chain.into_iter().filter(|&x| crate::templ::made_by_instantiation(w, x)).collect();
    let lists: Vec<Vec<AttrNode>> = match moved {
        Some(m) => made
            .iter()
            .map(|&x| own_nodes(w, s, x).into_iter().filter(|n| m.node(n)).collect())
            .collect(),
        None => Vec::new(),
    };
    groups(w, s, &lists);
    if moved == Some(Moved::All) {
        chain_annotations(w, s, &made);
    }
}

/// The position of a parameter that is not where its `ParamData` `index`
/// says in its function's parameter list.
const PARAM_INDEX_UNACCOUNTED: Cause = Cause::Unaccounted("ParamData/index");
/// A parameter of a declaration (not at block scope) that its function's
/// chain (`decl_func::param_decls`) does not hold.
const PARAM_CHAIN_UNACCOUNTED: Cause = Cause::Unaccounted("ParamData/chain");

/// The row `decl_attrs` of `AttrArena` (`attrs`, §10.1) and the rows of
/// `AnnotArena` (§10.5) for `d`. A parameter's are [`param_attrs`]; any
/// other declaration's are [`entity_attrs`] over its chain, first to last.
pub(crate) fn decl_attrs(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let ast = w.ast;
    if !param_attrs(w, s, d) {
        let mut chain = w.redecls(d);
        chain.reverse();
        let def = defining(w, d, &chain);
        // For an entity of the definition class, what Kiln writes onto a
        // declaration made by instantiation when it instantiates the
        // definition is `definition`'s (`moved_kinds`, §11.9); a node on
        // a declaration written in source stays here.
        let moved = if w.unit.definition { None } else { moved_kinds(w, w.entity(d)) };
        entity_attrs(w, s, &chain, def, moved);
    }
    // AnnotArena `of_base`: a list per base-specifier of the definition.
    let bases = match ast.definition(d) {
        Some(def) => ast.decls.bases(def, &ast.lists).count(),
        None => 0,
    };
    s.count(bases);
    let class = ast.first(d);
    for i in 0..bases {
        let anns = if let Ok(i) = u32::try_from(i) {
            ann_ids(w, s, ast.annots.of_base(class, i))
        } else {
            s.indeterminate(Cause::Unaccounted("AnnotArena/of_base"));
            Vec::new()
        };
        annotations(w, s, &anns);
    }
}

/// The defining declaration of entity `d` the attribute rules read
/// (§10.1): its definition, if `chain` holds it (a definition
/// instantiated outside its class is not the declared unit's, WP-032) and
/// the entity is not of the definition class (§11.9), whose definition
/// Kiln makes on use. A template's is the template its pattern's
/// definition describes.
fn defining(w: &Walk<'_>, d: DeclId, chain: &[DeclId]) -> Option<DeclId> {
    let ast = w.ast;
    // A template is not redeclarable itself: its chain is the templates
    // its pattern's declarations describe (`Walk::chain`), and its
    // definition the one its pattern's definition describes.
    let pattern = ast.template_info(d).and_then(|i| i.pattern);
    let def = match pattern {
        Some(p) if ast.decls.redecl(d).is_none() && ast.decls.redecl(p).is_some() => {
            ast.definition(p).and_then(|x| ast.described_template(x))
        }
        Some(_) | None => ast.definition(d),
    };
    def.filter(|x| chain.contains(x)).filter(|_| !crate::demand::definition_class(w, ast.first(d)))
}

/// The nodes of declaration `x`'s own list, in stored order, without the
/// `inherited` copies.
fn own_nodes(w: &Walk<'_>, s: &mut Stream, x: DeclId) -> Vec<AttrNode> {
    let ast = w.ast;
    attr_ids(w, s, ast.attrs.of_decl(x))
        .into_iter()
        .map(|a| *ast.attrs.get(a))
        .filter(|n| n.flags & INHERITED == 0)
        .collect()
}

/// The nodes of parameter `p` of declaration `x`: [`own_nodes`], except
/// for a declaration made by instantiation, whose parameter's nodes are
/// all written, `inherited` copies included. Since Kiln WP-3407
/// (`14b25bd70`, in the pin since WP-163) an instantiation's parameter
/// takes its attributes from the template's latest declaration, where an
/// attribute of the first declaration is an `inherited` copy; before, it
/// took them from the first declaration, as its own. Its chain holds no
/// other declaration with the attribute, so leaving the copy out would
/// lose it (parity: `differ/_/param-attr-template-first-declaration`).
fn param_nodes(w: &Walk<'_>, s: &mut Stream, x: DeclId, p: DeclId) -> Vec<AttrNode> {
    if !crate::templ::made_by_instantiation(w, x) {
        return own_nodes(w, s, p);
    }
    let ast = w.ast;
    attr_ids(w, s, ast.attrs.of_decl(p)).into_iter().map(|a| *ast.attrs.get(a)).collect()
}

/// `count` then `record AttrNode` for each of `nodes`.
fn node_list(w: &mut Walk<'_>, s: &mut Stream, nodes: &[AttrNode]) {
    s.count(nodes.len());
    for n in nodes {
        attr_node(w, s, n);
    }
}

/// `count` then each non-empty list of `lists`, in order, as a
/// [`node_list`].
fn groups(w: &mut Walk<'_>, s: &mut Stream, lists: &[Vec<AttrNode>]) {
    s.count(lists.iter().filter(|l| !l.is_empty()).count());
    for l in lists.iter().filter(|l| !l.is_empty()) {
        node_list(w, s, l);
    }
}

/// The annotations (`AnnotArena` `of_decl`) of every declaration of
/// `chain`, in its order, as one list: Kiln's `annotations_of` reads them
/// so (§10.5).
fn chain_annotations(w: &mut Walk<'_>, s: &mut Stream, chain: &[DeclId]) {
    let ast = w.ast;
    let mut anns: Vec<AnnId> = Vec::new();
    for &x in chain {
        anns.extend(ann_ids(w, s, ast.annots.of_decl(x)));
    }
    annotations(w, s, &anns);
}

/// `attrs` of an entity over the declarations `chain`, in its order
/// (§10.1), `def` its defining declaration: the lists of the declarations
/// before `def` (all of them when `def` is not in `chain`), empty lists
/// left out, preceded by their count; `opt`, present when `def` is in
/// `chain`, with its own list; the lists of the declarations after it as
/// those before; then the annotations as one list. What `moved` names
/// ([`moved_kinds`]) is left out of a declaration made by instantiation
/// (`templ::made_by_instantiation`).
fn entity_attrs(
    w: &mut Walk<'_>,
    s: &mut Stream,
    chain: &[DeclId],
    def: Option<DeclId>,
    moved: Option<Moved>,
) {
    let made: Vec<bool> = chain
        .iter()
        .map(|&x| moved.is_some() && crate::templ::made_by_instantiation(w, x))
        .collect();
    let lists: Vec<Vec<AttrNode>> = chain
        .iter()
        .zip(&made)
        .map(|(&x, &mx)| {
            let nodes = own_nodes(w, s, x);
            match moved.filter(|_| mx) {
                Some(m) => nodes.into_iter().filter(|n| !m.node(n)).collect(),
                None => nodes,
            }
        })
        .collect();
    let at = def.and_then(|x| chain.iter().position(|&y| y == x));
    let (before, rest) = lists.split_at(at.unwrap_or(lists.len()));
    groups(w, s, before);
    s.opt(at.is_some());
    let after = match rest.split_first() {
        Some((own, after)) => {
            node_list(w, s, own);
            after
        }
        None => rest,
    };
    groups(w, s, after);
    let kept: Vec<DeclId> = chain
        .iter()
        .zip(&made)
        .filter(|&(_, &mx)| !(moved == Some(Moved::All) && mx))
        .map(|(&x, _)| x)
        .collect();
    chain_annotations(w, s, &kept);
}

/// `attrs` of parameter `d` of a function (§10.1), or `false` when `d` is
/// no function's parameter. Over the parameters i of the declarations of
/// `decl_func::param_decls` of its function: `opt`, present when the
/// function has a defining declaration among them (and is not of the
/// definition class, §11.9), with that parameter's own nodes as one
/// list; then the other parameters' lists, in chain order, empty lists
/// left out, preceded by their count; then the annotations of all of
/// them as one list. A parameter of a block-scope declaration thus has
/// no attributes in the unit: they are written with that declaration in
/// its block ([`own_attrs`]).
fn param_attrs(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) -> bool {
    let ast = w.ast;
    let Some(index) = ast.decls.param(d).map(|p| usize::from(p.index)) else { return false };
    let Some(f) = ast.decls.get(d).ctx() else { return false };
    let Some(own) = ast.decls.function(f) else { return false };
    if ast.lists.iter::<DeclId>(own.params).nth(index) != Some(d) {
        s.indeterminate(PARAM_INDEX_UNACCOUNTED);
        s.opt(false);
        let nodes = own_nodes(w, s, d);
        groups(w, s, &[nodes]);
        chain_annotations(w, s, &[d]);
        return true;
    }
    let mut decls = crate::decl_func::param_decls(w, f);
    // The resolved unit `definition` reads the parameters of its defining
    // declaration, which the chain leaves out when an instantiation made
    // it outside its class (a friend's definition, §11.9): it is the last
    // declaration Kiln chained.
    if w.unit.definition && !decls.contains(&f) && crate::units::instantiated_outside_class(w, f) {
        decls.push(f);
    }
    if !decls.contains(&f) && !w.is_local_extern(f) {
        // The declaration whose parameter is written is not in the chain
        // read: its own nodes would be lost.
        s.indeterminate(PARAM_CHAIN_UNACCOUNTED);
    }
    let def = defining(w, f, &decls);
    // Parameter i of each declaration; C++ gives every declaration of a
    // function the same number of parameters.
    let params: Vec<(DeclId, DeclId)> = decls
        .iter()
        .filter_map(|&x| {
            let param = ast.lists.iter::<DeclId>(ast.decls.function(x)?.params).nth(index)?;
            Some((x, param))
        })
        .collect();
    let def_param = params.iter().find(|&&(x, _)| Some(x) == def).copied();
    s.opt(def_param.is_some());
    if let Some((x, p)) = def_param {
        let nodes = param_nodes(w, s, x, p);
        node_list(w, s, &nodes);
    }
    let others: Vec<Vec<AttrNode>> = params
        .iter()
        .filter(|&&(x, _)| Some(x) != def)
        .map(|&(x, p)| param_nodes(w, s, x, p))
        .collect();
    groups(w, s, &others);
    let chain: Vec<DeclId> = params.iter().map(|&(_, p)| p).collect();
    chain_annotations(w, s, &chain);
    true
}

/// The attributes and annotations parameter `d` of a block-scope function
/// declaration holds itself (`local::write_decl`, §2.4): its own nodes as
/// one `attrs` list (the `inherited` copies left out), then its
/// annotations.
pub(crate) fn own_attrs(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    let nodes = own_nodes(w, s, d);
    node_list(w, s, &nodes);
    chain_annotations(w, s, &[d]);
}

/// `attrs` of a statement (`AttributedStmt.a`, §10.1): the nodes of the
/// list `l`, in stored order.
#[allow(dead_code, reason = "called by the statements area, which lands later (lib.rs)")]
pub(crate) fn stmt_attrs(w: &mut Walk<'_>, s: &mut Stream, l: ListId) {
    let ast = w.ast;
    let ids = attr_ids(w, s, l);
    s.count(ids.len());
    for a in ids {
        let n = *ast.attrs.get(a);
        attr_node(w, s, &n);
    }
}

/// The ids of the attribute list `l`; a word that is no attribute makes
/// the stream indeterminate and is left out.
fn attr_ids(w: &Walk<'_>, s: &mut Stream, l: ListId) -> Vec<AttrId> {
    let ast = w.ast;
    let Some(words) = list_words(w, l) else {
        s.indeterminate(Cause::Unaccounted("AttrArena/decl_attrs"));
        return Vec::new();
    };
    let mut out = Vec::with_capacity(words.len());
    for &raw in words {
        match AttrId::from_raw(raw) {
            Some(a) if (raw as usize) <= ast.attrs.len() => out.push(a),
            _ => {
                s.indeterminate(Cause::Unaccounted("AttrArena/decl_attrs"));
            }
        }
    }
    out
}

/// The ids of the annotation list `l`.
fn ann_ids(w: &Walk<'_>, s: &mut Stream, l: ListId) -> Vec<AnnId> {
    let ast = w.ast;
    let Some(words) = list_words(w, l) else {
        s.indeterminate(Cause::Unaccounted("AnnotArena/of_decl"));
        return Vec::new();
    };
    let mut out = Vec::with_capacity(words.len());
    for &raw in words {
        match AnnId::from_raw(raw) {
            Some(a) if (raw as usize) <= ast.annots.len() => out.push(a),
            _ => {
                s.indeterminate(Cause::Unaccounted("AnnotArena/of_decl"));
            }
        }
    }
    out
}

/// The words of list `l`, or `None` if the arena has no such list.
fn list_words<'a>(w: &Walk<'a>, l: ListId) -> Option<&'a [u32]> {
    let ast = w.ast;
    // Index 0 is the dummy, 1 the empty list, then `count()` more.
    if l.index() as usize > ast.lists.count() + 1 {
        return None;
    }
    Some(ast.lists.get(l))
}

/// `record Annotation*` (§10.5).
fn annotations(w: &mut Walk<'_>, s: &mut Stream, anns: &[AnnId]) {
    let ast = w.ast;
    s.count(anns.len());
    for &a in anns {
        let Annotation { decl: _, value, loc: _, written_type, expr } = *ast.annots.get(a);
        // value: value?
        write_value_opt(w, s, value);
        // written_type: type
        w.write_type(s, written_type);
        // expr: child
        crate::expr::child(w, s, expr);
    }
}

fn write_value_opt(w: &mut Walk<'_>, s: &mut Stream, v: Option<ConstValueId>) {
    s.opt(v.is_some());
    if let Some(v) = v {
        crate::value::value(w, s, v);
    }
}

/// `record AttrNode` (§10.1, §2.1).
fn attr_node(w: &mut Walk<'_>, s: &mut Stream, n: &AttrNode) {
    let AttrNode { kind, syntax, flags, loc: _, a, b } = *n;
    // kind: enum AttrKind (a `consts` table: a value with no row is
    // unaccounted for).
    let ops: &[Operand] = if let Some(r) = kind_row(kind) {
        s.tag(r.row);
        if r.row.decision == Decision::Unsupported {
            s.indeterminate(Cause::Unsupported(r.name));
        }
        r.ops
    } else {
        s.uv(0).indeterminate(Cause::Unaccounted("AttrNode/kind"));
        &[]
    };
    // syntax: enum AttrSyntax.
    s.tag(attr_syntax::row(&syntax));
    // The words no row of the kind reads.
    let mut flag_mask = COMMON_FLAGS;
    let (mut a_mask, mut b_mask) = (0u32, 0u32);
    for o in ops {
        flag_mask |= o.enc.flag_mask();
        let (am, bm) = o.enc.word_masks();
        a_mask |= am;
        b_mask |= bm;
    }
    if flags & !flag_mask != 0 {
        s.indeterminate(Cause::Unaccounted("AttrNode/flags"));
    }
    // `flags-checked` (bit 6): every attribute Kiln leaves in a list is
    // checked once the translation unit is done (Kiln `spec/ast.md` §7
    // "Checked marker"); one that is not holds its words as written,
    // which no row describes.
    if flags & AttrNode::CHECKED == 0 {
        s.indeterminate(Cause::Unaccounted("AttrNode/flags-checked"));
    }
    if a & !a_mask != 0 {
        s.indeterminate(Cause::Unaccounted("AttrNode/a"));
    }
    if b & !b_mask != 0 {
        s.indeterminate(Cause::Unaccounted("AttrNode/b"));
    }
    // The flag word: `flags-implicit`, then the kind's `bit` rows.
    let mut word = FlagWord::new().bit(flags & AttrNode::IMPLICIT != 0);
    for o in ops {
        if let Enc::Bit(m) = o.enc {
            word = word.bit(flags & m != 0);
        }
    }
    s.flags(word);
    // The kind's operand rows, in table order.
    for o in ops {
        operand(w, s, *o, flags, a, b);
    }
}

/// One per-kind operand row of `AttrNode`.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "one arm per operand encoding; a and b are the AttrNode word names"
)]
fn operand(w: &mut Walk<'_>, s: &mut Stream, o: Operand, flags: u8, a: u32, b: u32) {
    let word = |x: Word| match x {
        Word::A => a,
        Word::B => b,
    };
    let bad = Cause::Unaccounted(o.name);
    match o.enc {
        // Written in the flag word, or not written.
        Enc::Bit(_) | Enc::OutBit(_) | Enc::Out(_) => {}
        Enc::OutBits(x, m) => {
            // The spelling's index of `NO_SANITIZE.b-spelling` names one
            // of Kiln's spellings.
            if m == NO_SANITIZE_B_SPELLING
                && ((word(x) & m) >> m.trailing_zeros()) as usize >= NO_SANITIZE_SPELLINGS.len()
            {
                s.indeterminate(bad);
            }
        }
        Enc::Ref(x) => match decl_word(w, word(x)) {
            Some(d) => w.write_ref(s, d),
            None => {
                s.uv(crate::refs::REF_NONE).indeterminate(bad);
            }
        },
        Enc::Child(x) => match expr_id(w, word(x)) {
            Some(e) => crate::expr::child(w, s, e),
            None => {
                s.uv(0).indeterminate(bad);
            }
        },
        Enc::ChildOpt(x) => child_opt_word(w, s, word(x), bad),
        Enc::ChildIfFlag(x, m) => {
            if flags & m != 0 {
                child_opt_word(w, s, word(x), bad);
            } else {
                s.opt(false);
            }
        }
        Enc::UvUnlessFlag(x, m) => {
            if flags & m == 0 {
                s.opt(true).uv(u64::from(word(x)));
            } else {
                s.opt(false);
            }
        }
        Enc::Children(x, zero_empty) => {
            let raw = word(x);
            if raw == 0 && zero_empty {
                s.count(0);
                return;
            }
            let words = ListId::from_raw(raw).and_then(|l| list_words(w, l));
            let Some(words) = words else {
                s.count(0).indeterminate(bad);
                return;
            };
            s.count(words.len());
            for &e in words {
                match expr_id(w, e) {
                    Some(e) => crate::expr::child(w, s, e),
                    None => {
                        s.uv(0).indeterminate(bad);
                    }
                }
            }
        }
        Enc::Type(x) => match type_word(w, word(x)) {
            Some(q) => w.write_type(s, q),
            None => {
                s.digest(&slip_model::Digest([0; 32])).indeterminate(bad);
            }
        },
        Enc::TypeOpt(x) => type_opt_word(w, s, word(x), bad),
        Enc::TypeIfFlag(x, m) => {
            let raw = word(x);
            if (flags & m != 0) != (raw != 0) {
                s.indeterminate(bad);
            }
            type_opt_word(w, s, raw, bad);
        }
        Enc::String(x) => match string_id(w, word(x)) {
            Some(l) => crate::value::string(w, s, l),
            None => {
                s.uv(0).indeterminate(bad);
            }
        },
        Enc::StringOpt(x) => {
            let raw = word(x);
            s.opt(raw != 0);
            if raw != 0 {
                match string_id(w, raw) {
                    Some(l) => crate::value::string(w, s, l),
                    None => {
                        s.uv(0).indeterminate(bad);
                    }
                }
            }
        }
        Enc::Uv(x) => {
            s.uv(u64::from(word(x)));
        }
        Enc::UvIn(x, lo, hi) => {
            let v = word(x);
            if v < lo || v > hi {
                s.indeterminate(bad);
            }
            s.uv(u64::from(v));
        }
        Enc::Uvs(x) => {
            let words = ListId::from_raw(word(x)).and_then(|l| list_words(w, l));
            let Some(words) = words else {
                s.count(0).indeterminate(bad);
                return;
            };
            s.count(words.len());
            for &v in words {
                s.uv(u64::from(v));
            }
        }
        Enc::Sym(x) => {
            let raw = word(x);
            if (raw as usize) < w.ast.symbols().len() {
                w.write_sym(s, Symbol::from_index(raw));
            } else {
                s.bytes(b"").indeterminate(bad);
            }
        }
        Enc::ParamAbi(x) => param_abi_word(s, word(x), bad),
        Enc::LoopHint(x) => loop_hint_word(s, word(x), bad),
        Enc::Sanitizers => sanitizers(s, no_sanitize_set(a, b)),
        Enc::BoolBits(x, m) => {
            s.bool(word(x) & m != 0);
        }
    }
}

/// `child?` of a raw `ExprId`, 0 absent.
fn child_opt_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, bad: Cause) {
    s.opt(raw != 0);
    if raw != 0 {
        match expr_id(w, raw) {
            Some(e) => crate::expr::child(w, s, e),
            None => {
                s.uv(0).indeterminate(bad);
            }
        }
    }
}

/// `type?` of a raw `QualType`, 0 absent.
fn type_opt_word(w: &mut Walk<'_>, s: &mut Stream, raw: u32, bad: Cause) {
    s.opt(raw != 0);
    if raw != 0 {
        match type_word(w, raw) {
            Some(q) => w.write_type(s, q),
            None => {
                s.digest(&slip_model::Digest([0; 32])).indeterminate(bad);
            }
        }
    }
}

/// The declaration with raw index `raw`, if the arena has it.
fn decl_word(w: &Walk<'_>, raw: u32) -> Option<DeclId> {
    DeclId::try_new(raw).filter(|_| raw as usize <= w.ast.decls.len())
}

/// The expression with raw index `raw`, if the arena has it.
fn expr_id(w: &Walk<'_>, raw: u32) -> Option<ExprId> {
    ExprId::from_raw(raw).filter(|_| raw as usize <= w.ast.exprs.len())
}

/// The string literal with raw index `raw`, if the pool has it.
fn string_id(w: &Walk<'_>, raw: u32) -> Option<StringLitId> {
    StringLitId::from_raw(raw).filter(|_| raw as usize <= w.ast.strings.len())
}

/// The `QualType` with raw encoding `raw`, if its type exists.
fn type_word(w: &Walk<'_>, raw: u32) -> Option<QualType> {
    QualType::from_raw(raw).filter(|q| q.ty().index() as usize <= w.ast.types.len())
}

/// `enum ParamAbi` of a raw word (`SWIFT_PARAM_ABI.a`).
fn param_abi_word(s: &mut Stream, raw: u32, bad: Cause) {
    let abi = u8::try_from(raw).ok().map(ParamAbi::from_u8).filter(|&p| p as u32 == raw);
    let Some(abi) = abi else {
        s.uv(0).indeterminate(bad);
        return;
    };
    s.tag(match abi {
        ParamAbi::Ordinary => param_abi::ORDINARY,
        ParamAbi::SwiftContext => param_abi::SWIFT_CONTEXT,
        ParamAbi::SwiftAsyncContext => param_abi::SWIFT_ASYNC_CONTEXT,
        ParamAbi::SwiftErrorResult => param_abi::SWIFT_ERROR_RESULT,
        ParamAbi::SwiftIndirectResult => param_abi::SWIFT_INDIRECT_RESULT,
    });
}

/// `record LoopHint` of a packed word (`LOOP_HINT.b`): a word that does
/// not unpack to itself holds a value no row names.
fn loop_hint_word(s: &mut Stream, raw: u32, bad: Cause) {
    let hint = LoopHint::unpack(raw);
    if hint.pack() != raw {
        s.indeterminate(bad);
    }
    let LoopHint { option, state, spelling } = hint;
    s.tag(loop_hint_option::row(&option));
    s.tag(loop_hint_state::row(&state));
    s.tag(loop_hint_spelling::row(&spelling));
}

/// `record SanitizerSet`: the `-fsanitize=` names of its members,
/// `bytes*sorted`. A bit with no name is unaccounted for.
fn sanitizers(s: &mut Stream, set: SanitizerSet) {
    if set.0 & !SanitizerSet::ALL.0 != 0 {
        s.indeterminate(Cause::Unaccounted("SanitizerSet/0"));
    }
    let mut names: Vec<Vec<u8>> = set
        .names()
        .map(|n| {
            let mut one = Stream::new();
            one.bytes(n.as_bytes());
            one.as_bytes().to_vec()
        })
        .collect();
    names.sort();
    names.dedup();
    s.count(names.len());
    for n in &names {
        s.raw(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{attr_node, string_encoding, value_kind};

    /// The constants of a module of `tables.rs`: `(name, tag, decision)`.
    fn generated_consts(module: &str) -> Vec<(String, u32, String)> {
        let src = include_str!("tables.rs");
        let start = format!("pub(crate) mod {module} {{");
        let body = src.split_once(&start).expect("the module is generated").1;
        let body = body.split_once("\n}\n").expect("the module ends").0;
        let mut out = Vec::new();
        for line in body.lines() {
            let Some(rest) = line.trim().strip_prefix("pub(crate) const ") else { continue };
            let Some((name, rhs)) = rest.split_once(": Row = Row { tag: ") else { continue };
            let (tag, rhs) = rhs.split_once(", decision: Decision::").expect("a row");
            let decision = rhs.trim_end_matches(" };").to_owned();
            out.push((name.to_owned(), tag.parse().expect("a tag"), decision));
        }
        out
    }

    fn decision_name(d: Decision) -> &'static str {
        match d {
            Decision::In => "In",
            Decision::Out => "Out",
            Decision::Symbolic => "Symbolic",
            Decision::Demand => "Demand",
            Decision::Unsupported => "Unsupported",
        }
    }

    // §10.1 `AttrKind`: `KINDS` holds every row of the generated table
    // once, in table order, with its tag and decision.
    #[test]
    fn kinds_are_the_generated_rows() {
        let want = generated_consts("attr_kind");
        assert_eq!(want.len(), 147, "the AttrKind rows of tables.rs were not found");
        let got: Vec<(String, u32, String)> = KINDS
            .iter()
            .map(|r| {
                let name = r.name.strip_prefix("AttrKind/").expect("a test name").to_owned();
                (name, r.row.tag, decision_name(r.row.decision).to_owned())
            })
            .collect();
        assert_eq!(got, want);
    }

    // §10.1 `AttrKind`: each row names the Kiln constant of its name, and
    // every constant of Kiln's `attr.rs` has a row.
    #[test]
    fn kinds_are_kilns_constants() {
        let src = include_str!("../../../kiln/crates/kiln_ast/src/attr.rs");
        let mut kiln = Vec::new();
        for line in src.lines() {
            let Some(rest) = line.trim().strip_prefix("pub const ") else { continue };
            let Some((name, rhs)) = rest.split_once(": AttrKind = AttrKind(") else { continue };
            let n: u16 = rhs.trim_end_matches(");").parse().expect("an AttrKind number");
            kiln.push((name.to_owned(), n));
        }
        assert!(kiln.len() > 140, "the AttrKind constants were not found");
        for (name, n) in &kiln {
            let row = KINDS
                .iter()
                .find(|r| r.name.strip_prefix("AttrKind/") == Some(name.as_str()))
                .unwrap_or_else(|| panic!("AttrKind::{name} has no row"));
            assert_eq!(row.kind, AttrKind(*n), "AttrKind::{name}");
        }
        assert_eq!(kiln.len(), KINDS.len(), "a row names no Kiln constant");
    }

    // §10.1 `AttrNode` `DIAGNOSE_IF.flags-*`: the bits are Kiln's private
    // constants (`kiln_sema/src/diagnose_if.rs`), read here by text so a
    // renumbering at the next pin fails this test.
    #[test]
    fn diagnose_if_bits_are_kilns_constants() {
        let src = include_str!("../../../kiln/crates/kiln_sema/src/diagnose_if.rs");
        let kiln = |name: &str| -> u8 {
            let head = format!("pub(crate) const {name}: u8 = ");
            let mut found = src.lines().filter_map(|l| l.trim().strip_prefix(head.as_str()));
            let v = found.next().unwrap_or_else(|| panic!("{name} not found"));
            assert!(found.next().is_none(), "{name} defined twice");
            v.trim_end_matches(';').parse().expect("a u8")
        };
        assert_eq!(kiln("DIAGNOSE_IF_ERROR"), DIAGNOSE_IF_ERROR);
        assert_eq!(kiln("DIAGNOSE_IF_ARG_DEPENDENT"), DIAGNOSE_IF_ARG_DEPENDENT);
        assert_eq!(kiln("DIAGNOSE_IF_GROUP"), DIAGNOSE_IF_GROUP);
    }

    // §10.1 `AttrNode` `flags-inherited`: bit 1, as the doc comment of
    // Kiln's `AttrNode::flags` says, and as its one setting site (the
    // inherited asm label, `kiln_sema/src/decl.rs`) sets it.
    #[test]
    fn inherited_bit_is_kilns() {
        let attr = include_str!("../../../kiln/crates/kiln_ast/src/attr.rs");
        let doc = attr
            .split_once("/// Per-kind flags (")
            .and_then(|(_, rest)| rest.split_once("pub flags: u8"))
            .map(|(doc, _)| doc.replace("///", " "))
            .expect("the doc comment of AttrNode::flags");
        let bit = doc
            .split_once(": inherited")
            .and_then(|(before, _)| before.rsplit_once("bit "))
            .and_then(|(_, n)| n.trim().parse::<u32>().ok())
            .expect("a `bit N: inherited` clause");
        assert_eq!(1u8 << bit, INHERITED, "the doc comment");
        let decl = include_str!("../../../kiln/crates/kiln_sema/src/decl.rs");
        let sites: Vec<usize> = decl.match_indices("node.flags |= ").map(|(i, _)| i).collect();
        let site = decl[..sites[0]].rfind("AttrKind::ASM_LABEL").map(|i| &decl[i..]);
        assert_eq!(sites.len(), 1, "one setting site of an attribute's flags in decl.rs");
        let set = site.and_then(|t| t.split_once("node.flags |= ")).map(|(_, r)| r);
        let set = set.and_then(|r| r.split_once(';')).map(|(v, _)| v.trim());
        assert_eq!(set, Some(INHERITED.to_string().as_str()), "the asm-label site");
    }

    // §10.1 `AttrNode`: the per-kind rows written here are those of the
    // generated table, kind by kind in table order, with its decisions;
    // the rows without a kind are the ones `attr_node` writes itself.
    #[test]
    fn operands_are_the_generated_rows() {
        let unprefixed: Vec<&str> =
            attr_node::ROWS.iter().map(|(r, _)| *r).filter(|r| !r.contains('.')).collect();
        assert_eq!(
            unprefixed,
            [
                "kind",
                "syntax",
                "flags-implicit",
                "flags-inherited",
                "flags-gnu-scoped",
                "flags-checked",
                "loc"
            ]
        );
        let mut seen = 0;
        for k in KINDS {
            let kind = k.name.strip_prefix("AttrKind/").expect("a test name");
            let want: Vec<(&str, Decision)> = attr_node::ROWS
                .iter()
                .filter(|(r, _)| r.split_once('.').is_some_and(|(p, _)| p == kind))
                .copied()
                .collect();
            let got: Vec<(&str, Decision)> = k
                .ops
                .iter()
                .map(|o| (o.name.strip_prefix("AttrNode/").expect("a test name"), o.enc.decision()))
                .collect();
            assert_eq!(got, want, "the AttrNode rows of {kind}");
            seen += want.len();
        }
        let prefixed = attr_node::ROWS.iter().filter(|(r, _)| r.contains('.')).count();
        assert_eq!(seen, prefixed, "a per-kind row names no AttrKind row");
    }

    // §10.1: a kind's flag bits are distinct from each other and from the
    // common ones, so that each row reads its own bit.
    #[test]
    fn flag_bits_are_distinct() {
        for k in KINDS {
            let mut used = COMMON_FLAGS;
            for o in k.ops {
                let m = o.enc.flag_mask();
                assert_eq!(used & m, 0, "{}: a flag bit read twice", o.name);
                used |= m;
            }
        }
    }

    /// Parses `src` and hands `f` a walk over it.
    fn with_walk<R>(src: &str, std: &str, f: impl FnOnce(&mut Walk<'_>) -> R) -> R {
        let files = [("main.cpp", src)];
        let snippet =
            slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
        slip_kiln::snippet::parse(&snippet, |p| {
            assert_eq!(p.errors, 0, "{}", p.diagnostics);
            let path = |_| Vec::new();
            let input = crate::Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let rules = slip_model::keys::RuleTable::empty();
            let mut w = Walk::new(&input, &rules);
            f(&mut w)
        })
        .expect("the snippet parses")
    }

    /// The last declaration named `name`.
    fn named(w: &Walk<'_>, name: &str) -> DeclId {
        let ast = w.ast;
        (1..=u32::try_from(ast.decls.len()).expect("few declarations"))
            .rev()
            .map(DeclId::new)
            .find(|&d| {
                ast.decls.get(d).name().as_ident().is_some_and(|s| ast.symbols().as_str(s) == name)
            })
            .unwrap_or_else(|| panic!("no declaration {name}"))
    }

    /// What `decl_attrs` writes for the declaration `name` of `src`.
    fn decl_bytes(src: &str, std: &str, name: &str) -> (Vec<u8>, Option<Cause>) {
        with_walk(src, std, |w| {
            let d = named(w, name);
            let mut s = Stream::new();
            decl_attrs(w, &mut s, d);
            (s.as_bytes().to_vec(), s.cause())
        })
    }

    /// What `stmt_attrs` writes for the first `AttributedStmt` of `src`.
    fn stmt_bytes(src: &str, std: &str) -> (Vec<u8>, Option<Cause>) {
        use slip_kiln::kiln::ast::StmtId;
        use slip_kiln::kiln::ast::stmt::StmtKind;
        with_walk(src, std, |w| {
            let ast = w.ast;
            let st = (1..=u32::try_from(ast.stmts.len()).expect("few statements"))
                .map(StmtId::new)
                .find(|&x| ast.stmts.get(x).kind() == StmtKind::AttributedStmt)
                .expect("an attributed statement");
            let mut s = Stream::new();
            stmt_attrs(w, &mut s, ast.stmts.stmt_attrs(st));
            (s.as_bytes().to_vec(), s.cause())
        })
    }

    /// What a test reads, and the bytes of each expression operand.
    type WithOps = ((Vec<u8>, Option<Cause>), Vec<Vec<u8>>);

    /// `decl_bytes`, and the bytes of each expression operand (what
    /// `expr::child` wrote for it), in order.
    fn decl_ops(src: &str, std: &str, name: &str) -> WithOps {
        crate::expr::capture::captured(|| decl_bytes(src, std, name))
    }

    /// `stmt_bytes`, and the bytes of each expression operand.
    fn stmt_ops(src: &str, std: &str) -> WithOps {
        crate::expr::capture::captured(|| stmt_bytes(src, std))
    }

    /// What `node` alone writes, over the walk of an empty source.
    fn node_cause(node: AttrNode) -> Option<Cause> {
        with_walk("", "c++20", |w| {
            let mut s = Stream::new();
            attr_node(w, &mut s, &node);
            s.cause()
        })
    }

    /// A node as Kiln leaves it in a list: checked (`flags-checked`).
    fn node(kind: AttrKind, flags: u8, a: u32, b: u32) -> AttrNode {
        unchecked(kind, flags | AttrNode::CHECKED, a, b)
    }

    fn unchecked(kind: AttrKind, flags: u8, a: u32, b: u32) -> AttrNode {
        AttrNode {
            kind,
            syntax: slip_kiln::kiln::ast::attr::AttrSyntax::Gnu,
            flags,
            loc: slip_kiln::kiln::source::Loc::INVALID,
            a,
            b,
        }
    }

    /// The bytes `f` writes.
    fn bytes(f: impl FnOnce(&mut Stream)) -> Vec<u8> {
        let mut s = Stream::new();
        f(&mut s);
        s.as_bytes().to_vec()
    }

    /// The start of one node: its kind, its syntax and its flag word.
    fn head(s: &mut Stream, kind: Row, syntax: Row, bits: &[bool]) {
        s.tag(kind).tag(syntax);
        let mut word = FlagWord::new();
        for &b in bits {
            word = word.bit(b);
        }
        s.flags(word);
    }

    /// The start of the lists of an entity of one declaration that is not
    /// a definition, with `n` nodes: one list before the (absent)
    /// definition.
    fn declared(s: &mut Stream, n: usize) {
        s.count(1).count(n);
    }

    /// The end of the lists of an entity [`declared`]: no definition, no
    /// list after it, no annotation, no base.
    fn no_annotations(s: &mut Stream) {
        s.opt(false).count(0).count(0).count(0);
    }

    /// The start of the lists of an entity of one declaration that is a
    /// definition, with `n` nodes: no list before it, then its own.
    fn defined(s: &mut Stream, n: usize) {
        s.count(0).opt(true).count(n);
    }

    /// The end of the lists of an entity [`defined`]: no list after the
    /// definition, no annotation, no base.
    fn defined_end(s: &mut Stream) {
        s.count(0).count(0).count(0);
    }

    use attr_syntax::{CXX11, GNU, KEYWORD, PRAGMA};

    // §10.1 `DEPRECATED.a`, `NODISCARD.a`: the message is its expression,
    // after the presence byte (the content is what `expr::child` writes).
    #[test]
    fn s10_1_message_is_an_expression_operand() {
        let (got, ops) = decl_ops(r#"[[deprecated("m")]] void f();"#, "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::DEPRECATED, CXX11, &[false]);
            s.opt(true).raw(&ops[0]).opt(false);
            no_annotations(s);
        });
        assert_eq!(got.0, want);
        let ((got, _), ops) = decl_ops(r#"[[nodiscard("m")]] int f();"#, "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::NODISCARD, CXX11, &[false]);
            s.opt(true).raw(&ops[0]).opt(false);
            no_annotations(s);
        });
        assert_eq!(got, want);
    }

    // §10.1 `ALLOC_SIZE.a`, `ALLOC_ALIGN.a`, `ENABLE_IF.a`, `ENABLE_IF.b`:
    // operands that are expressions, written through `expr::child`.
    #[test]
    fn s10_1_expression_operands() {
        let ((got, _), ops) =
            decl_ops("void *f(int) __attribute__((alloc_size(1)));", "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::ALLOC_SIZE, GNU, &[false]);
            s.opt(true).raw(&ops[0]).opt(false);
            no_annotations(s);
        });
        assert_eq!(got, want);
        let ((got, _), ops) =
            decl_ops("void *f(int) __attribute__((alloc_align(1)));", "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::ALLOC_ALIGN, GNU, &[false]);
            s.opt(true).raw(&ops[0]).opt(false);
            no_annotations(s);
        });
        assert_eq!(got, want);
        let src = r#"void f(int x) __attribute__((enable_if(x > 0, "m")));"#;
        let ((got, _), ops) = decl_ops(src, "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::ENABLE_IF, GNU, &[false]);
            s.opt(true).raw(&ops[0]).opt(true).raw(&ops[1]);
            no_annotations(s);
        });
        assert_eq!(got, want);
    }

    // §10.1 `DIAGNOSE_IF`: its three bits in table order after
    // `flags-implicit`, then the condition and the message.
    #[test]
    fn s10_1_diagnose_if() {
        let src = r#"void f(int x) __attribute__((diagnose_if(x < 0, "m", "error")));"#;
        let ((got, _), ops) = decl_ops(src, "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::DIAGNOSE_IF, GNU, &[false, true, true, false]);
            s.opt(true).raw(&ops[0]).opt(true).raw(&ops[1]);
            no_annotations(s);
        });
        assert_eq!(got, want);
    }

    // §10.1 `ANNOTATE.a`, `ANNOTATE.b`: the string, then the extra
    // arguments (0, the empty list, written as a count of 0).
    #[test]
    fn s10_1_annotate() {
        let ((got, _), ops) = decl_ops(r#"[[clang::annotate("x")]] void f();"#, "c++20", "f");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::ANNOTATE, CXX11, &[false]);
            s.opt(true).raw(&ops[0]).count(0);
            no_annotations(s);
        });
        assert_eq!(got, want);
    }

    // §10.1 `CONSTRUCTOR.flags-priority-dependent`, `.a-priority`,
    // `.a-expression`: a dependent priority is the expression.
    #[test]
    fn s10_1_constructor_priority() {
        let src = "template <int N> struct X { __attribute__((constructor(N))) static void c(); };";
        let ((got, _), ops) = decl_ops(src, "c++20", "c");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::CONSTRUCTOR, GNU, &[false, true]);
            s.opt(false).opt(true).raw(&ops[0]);
            no_annotations(s);
        });
        assert_eq!(got, want);
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::DESTRUCTOR, GNU, &[false, false]);
            s.opt(true).uv(101).opt(false);
            no_annotations(s);
        });
        assert_eq!(decl_bytes("__attribute__((destructor(101))) void c();", "c++20", "c").0, want);
    }

    // §10.1 `ASM_LABEL.a`, `SECTION.a`, `TARGET.a`, `OMP_ASSUME.a`,
    // `ALIAS.a`, `IFUNC.a`: string operands, written by `value::string`.
    #[test]
    fn s10_1_string_operands() {
        let one = |src: &str, name: &str, kind: Row, syntax: Row, opt: bool, text: &[u8]| {
            let want = bytes(|s| {
                declared(s, 1);
                head(s, kind, syntax, &[false]);
                if opt {
                    s.opt(true);
                }
                // `string` (§10.3): the code units, then the encoding.
                s.bytes(text).tag(string_encoding::ORDINARY);
                no_annotations(s);
            });
            assert_eq!(decl_bytes(src, "c++20", name), (want, None));
        };
        one(r#"void f() asm("g");"#, "f", attr_kind::ASM_LABEL, KEYWORD, false, b"g");
        one(r#"int v __attribute__((section("s")));"#, "v", attr_kind::SECTION, GNU, true, b"s");
        one(
            r#"__attribute__((target("avx"))) void f();"#,
            "f",
            attr_kind::TARGET,
            GNU,
            true,
            b"avx",
        );
        one(
            r#"[[omp::assume("ompx_a")]] void f();"#,
            "f",
            attr_kind::OMP_ASSUME,
            CXX11,
            true,
            b"ompx_a",
        );
        one(
            r#"extern "C" void h() {} extern "C" void g() __attribute__((alias("h")));"#,
            "g",
            attr_kind::ALIAS,
            GNU,
            false,
            b"h",
        );
        one(
            r#"extern "C" void (*r())() { return 0; } extern "C" void g() __attribute__((ifunc("r")));"#,
            "g",
            attr_kind::IFUNC,
            GNU,
            false,
            b"r",
        );
    }

    // §10.1 `PRAGMA_CLANG_SECTION.a`, `.b`, `flags-implicit`: the
    // section a pragma gives (Kiln spells the node `Gnu`).
    #[test]
    fn s10_1_pragma_clang_section() {
        let src = "#pragma clang section data = \"s\"\nint v = 1;";
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::PRAGMA_CLANG_SECTION, GNU, &[true]);
            s.opt(true).bytes(b"s").tag(string_encoding::ORDINARY).uv(1);
            no_annotations(s);
        });
        assert_eq!(CLANG_SECTION_KINDS[1], "data");
        assert_eq!(decl_bytes(src, "c++20", "v").0, want);
    }

    // §10.1 the encoding `attrs`: a declaration's nodes over its chain,
    // first to last, the `inherited` copy left out.
    #[test]
    fn s10_1_chain_order() {
        let src = "__attribute__((cold)) void f();\n__attribute__((noinline)) void f();";
        let want = bytes(|s| {
            s.count(2).count(1);
            head(s, attr_kind::COLD, GNU, &[false]);
            s.count(1);
            head(s, attr_kind::NOINLINE, GNU, &[false]);
            no_annotations(s);
        });
        assert_eq!(decl_bytes(src, "c++20", "f"), (want, None));
        let src = "void f() asm(\"g\");\nvoid f();";
        with_walk(src, "c++20", |w| {
            let ast = w.ast;
            let last = named(w, "f");
            let copies: Vec<AttrNode> = ast
                .lists
                .iter::<AttrId>(ast.attrs.of_decl(last))
                .map(|a| *ast.attrs.get(a))
                .collect();
            assert!(
                copies.iter().all(|n| n.flags & INHERITED != 0),
                "Kiln copies the label to the second declaration as inherited: {copies:?}"
            );
            assert!(!copies.is_empty(), "Kiln no longer copies the asm label");
        });
        let one = decl_bytes("void f() asm(\"g\");", "c++20", "f");
        assert_eq!(decl_bytes(src, "c++20", "f"), one);
    }

    // §10.1 the encoding `attrs`, for a statement; `LOOP_HINT.a`,
    // `LOOP_HINT.b`, `LoopHint` (its three enumerations in row order).
    #[test]
    fn s10_1_loop_hints() {
        let src = "void f() {\n#pragma clang loop unroll(enable)\n for (;;) { }\n}";
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::LOOP_HINT, PRAGMA, &[false]);
            s.opt(false);
            s.tag(loop_hint_option::UNROLL).tag(loop_hint_state::ENABLE);
            s.tag(loop_hint_spelling::CLANG_LOOP);
        });
        assert_eq!(stmt_bytes(src, "c++20"), (want, None));
        let src = "void f() {\n#pragma unroll 4\n for (;;) { }\n}";
        let ((got, _), ops) = stmt_ops(src, "c++20");
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::LOOP_HINT, PRAGMA, &[false]);
            s.opt(true).raw(&ops[0]);
            s.tag(loop_hint_option::UNROLL_COUNT).tag(loop_hint_state::NUMERIC);
            s.tag(loop_hint_spelling::UNROLL);
        });
        assert_eq!(got, want);
        let src = "void f() {\n#pragma clang loop vectorize(assume_safety)\n for (;;) { }\n}";
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::LOOP_HINT, PRAGMA, &[false]);
            s.opt(false);
            s.tag(loop_hint_option::VECTORIZE).tag(loop_hint_state::ASSUME_SAFETY);
            s.tag(loop_hint_spelling::CLANG_LOOP);
        });
        assert_eq!(stmt_bytes(src, "c++20"), (want, None));
    }

    // §10.1 statement attributes: `ASSUME` (`flags-ignored`, `a`),
    // `FALLTHROUGH`, `LIKELY`, `CODE_ALIGN.a`.
    #[test]
    fn s10_1_statement_attributes() {
        let ((got, _), ops) = stmt_ops("void f(int x) { [[assume(x > 0)]]; }", "c++23");
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::ASSUME, CXX11, &[false, false]);
            s.raw(&ops[0]);
        });
        assert_eq!(got, want);
        let src = "int g(); void f() { [[assume(g() > 0)]]; }";
        let ((got, _), ops) = stmt_ops(src, "c++23");
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::ASSUME, CXX11, &[false, true]);
            s.raw(&ops[0]);
        });
        assert_eq!(got, want);
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::FALLTHROUGH, CXX11, &[false]);
        });
        let src = "void f(int x) { switch (x) { case 0: [[fallthrough]]; case 1: break; } }";
        assert_eq!(stmt_bytes(src, "c++20"), (want, None));
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::LIKELY, CXX11, &[false]);
        });
        assert_eq!(stmt_bytes("void f(int x) { if (x) [[likely]] { } }", "c++20"), (want, None));
        let want = bytes(|s| {
            s.count(1);
            head(s, attr_kind::CODE_ALIGN, CXX11, &[false]);
            s.uv(16);
        });
        let src = "void f() { [[clang::code_align(16)]] for (;;) { } }";
        assert_eq!(stmt_bytes(src, "c++20"), (want, None));
    }

    // §10.5 `AnnotArena` `of_decl`, `Annotation` `value`, `written_type`,
    // `expr`: the value (`value`, here an `Int`), the type, the expression
    // (what `expr::child` writes); `of_base` is a count of the bases.
    #[test]
    fn s10_5_annotations() {
        let ((got, _), ops) = decl_ops("[[=1]] void f();", "c++26", "f");
        let start = bytes(|s| {
            s.count(0).opt(false).count(0).count(1).opt(true).tag(value_kind::INT);
        });
        assert!(got.starts_with(&start), "{got:?}");
        let end = bytes(|s| {
            s.raw(&ops[0]).count(0);
        });
        assert!(got.ends_with(&end), "{got:?}");
        // The same value: the bytes differ in the expression only.
        let ((other, _), other_ops) = decl_ops("[[=1 + 0]] void f();", "c++26", "f");
        let cut = |b: &[u8], op: &[u8]| b[..b.len() - op.len() - 1].to_vec();
        assert_eq!(cut(&other, &other_ops[0]), cut(&got, &ops[0]));
        assert_ne!(other, got);
        let src = "struct Base {}; struct Base2 {}; struct D : Base, [[=1]] Base2 {};";
        let (got, _) = decl_bytes(src, "c++26", "D");
        let start = bytes(|s| {
            defined(s, 0);
            s.count(0).count(0).count(2).count(0).count(1).opt(true).tag(value_kind::INT);
        });
        assert!(got.starts_with(&start), "{got:?}");
    }

    // §10.1 the `bit` rows whose pair tests also change another row:
    // `DESTRUCTOR.flags-priority-dependent`, `MODE.flags-deferred` (Kiln
    // keeps the node only while it is set), `GSL_OWNER.flags-has-type`,
    // `GSL_POINTER.flags-has-type`.
    #[test]
    fn s10_1_bits_with_their_operands() {
        let src = "template <int N> struct X { __attribute__((destructor(N))) static void c(); };";
        let ((got, _), ops) = decl_ops(src, "c++20", "c");
        let want = bytes(|s| {
            declared(s, 1);
            head(s, attr_kind::DESTRUCTOR, GNU, &[false, true]);
            s.opt(false).opt(true).raw(&ops[0]);
            no_annotations(s);
        });
        assert_eq!(got, want);
        let src = "template <class T> struct X { T v __attribute__((mode(DI))); };";
        let (got, _) = decl_bytes(src, "c++20", "v");
        let start = bytes(|s| {
            defined(s, 1);
            head(s, attr_kind::MODE, GNU, &[false, true]);
        });
        assert!(got.starts_with(&start), "{got:?}");
        for (src, kind) in [
            ("struct [[gsl::Owner]] S {};", attr_kind::GSL_OWNER),
            ("struct [[gsl::Pointer]] S {};", attr_kind::GSL_POINTER),
        ] {
            let want = bytes(|s| {
                defined(s, 1);
                head(s, kind, CXX11, &[false, false]);
                s.opt(false);
                defined_end(s);
            });
            assert_eq!(decl_bytes(src, "c++20", "S"), (want, None), "{src}");
        }
        for (src, kind) in [
            ("struct [[gsl::Owner(int)]] S {};", attr_kind::GSL_OWNER),
            ("struct [[gsl::Pointer(int)]] S {};", attr_kind::GSL_POINTER),
        ] {
            let (got, cause) = decl_bytes(src, "c++20", "S");
            let start = bytes(|s| {
                defined(s, 1);
                head(s, kind, CXX11, &[false, true]);
                s.opt(true);
            });
            assert!(got.starts_with(&start), "{src}: {got:?}");
            assert_eq!(cause, None, "{src}");
        }
    }

    // §10.1 `BUILTIN`, `BUILTIN.a` (`out`), `flags-implicit`: a lazily
    // declared builtin's implicit attributes, in stored order.
    #[test]
    fn s10_1_builtin() {
        let src = "int g() { return __builtin_abs(1); }";
        let want = bytes(|s| {
            declared(s, 3);
            head(s, attr_kind::BUILTIN, GNU, &[true]);
            head(s, attr_kind::NOTHROW, GNU, &[true]);
            head(s, attr_kind::CONST, GNU, &[true]);
            no_annotations(s);
        });
        assert_eq!(decl_bytes(src, "c++20", "__builtin_abs"), (want, None));
    }

    // §10.1 AttrKind `X86_INTERRUPT`: Kiln adds an implicit `used` with it
    // (kiln_sema/src/imported_used.rs), so the differ pair cannot isolate
    // the node; here the interrupt node is pinned itself.
    #[test]
    fn s10_1_x86_interrupt() {
        let src = "struct F; __attribute__((interrupt)) void f(F *);";
        let want = bytes(|s| {
            declared(s, 2);
            head(s, attr_kind::X86_INTERRUPT, GNU, &[false]);
            head(s, attr_kind::USED, GNU, &[true]);
            no_annotations(s);
        });
        assert_eq!(decl_bytes(src, "c++20", "f"), (want, None));
    }

    // §10.1 `CLEANUP.a`: once checked (Kiln `AttrNode::CHECKED`) the
    // word is the cleanup function's `DeclId`, written as its `ref`.
    #[test]
    fn s10_1_cleanup_is_the_function() {
        let one = "void c(int *); void d(int *); void f() { int x __attribute__((cleanup(c))); }";
        let two = "void c(int *); void d(int *); void f() { int x __attribute__((cleanup(d))); }";
        let (a, ca) = decl_bytes(one, "c++20", "x");
        let (b, cb) = decl_bytes(two, "c++20", "x");
        assert_eq!((ca, cb), (None, None));
        assert_ne!(a, b);
        // The word is written as the `ref` of the function it names, not of
        // another declaration: `x`'s bytes hold the `ref` of `c` (or `d`)
        // and not that of the other.
        let refs = |src: &str| {
            with_walk(src, "c++20", |w| {
                let mut out = Vec::new();
                for n in ["c", "d"] {
                    let d = named(w, n);
                    let mut s = Stream::new();
                    w.write_ref(&mut s, d);
                    out.push(s.as_bytes().to_vec());
                }
                out
            })
        };
        let holds = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|x| x == needle);
        let (rc, rd) = match refs(one).as_slice() {
            [c, d] => (c.clone(), d.clone()),
            _ => unreachable!("two names"),
        };
        assert_ne!(rc, rd);
        assert!(holds(&a, &rc) && !holds(&a, &rd), "cleanup(c) writes the ref of c");
        assert!(holds(&b, &rd) && !holds(&b, &rc), "cleanup(d) writes the ref of d");
    }

    // §10.1 (and §2.1): every word is accounted for. A flag bit, an `a` or
    // a `b` no row of the kind names, a value outside a row's note, or a
    // kind with no row, is `Unaccounted`; an `unsupported` kind is
    // `Unsupported`; none panics.
    #[test]
    fn s10_1_unaccounted_words() {
        let un = |n| Some(Cause::Unaccounted(n));
        assert_eq!(node_cause(node(AttrKind::NORETURN, 0, 0, 0)), None);
        assert_eq!(node_cause(node(AttrKind::NORETURN, INHERITED | 8 | 1, 0, 0)), None);
        assert_eq!(node_cause(node(AttrKind::NORETURN, 4, 0, 0)), un("AttrNode/flags"));
        assert_eq!(node_cause(node(AttrKind::NORETURN, 0x80, 0, 0)), un("AttrNode/flags"));
        // `flags-checked` clear: a node as written.
        assert_eq!(
            node_cause(unchecked(AttrKind::NORETURN, 0, 0, 0)),
            un("AttrNode/flags-checked")
        );
        assert_eq!(node_cause(node(AttrKind::NORETURN, 0, 1, 0)), un("AttrNode/a"));
        assert_eq!(node_cause(node(AttrKind::NORETURN, 0, 0, 1)), un("AttrNode/b"));
        assert_eq!(node_cause(node(AttrKind::X86_INTERRUPT, 0, 1, 0)), un("AttrNode/a"));
        assert_eq!(node_cause(node(AttrKind::SECTION, 0, 0, 1)), un("AttrNode/b"));
        assert_eq!(node_cause(node(AttrKind(9999), 0, 0, 0)), un("AttrNode/kind"));
        assert_eq!(node_cause(node(AttrKind::VISIBILITY, 0, 2, 1)), None);
        assert_eq!(node_cause(node(AttrKind::VISIBILITY, 0, 3, 0)), un("AttrNode/VISIBILITY.a"));
        assert_eq!(node_cause(node(AttrKind::TLS_MODEL, 0, 0, 0)), un("AttrNode/TLS_MODEL.a"));
        assert_eq!(node_cause(node(AttrKind::TLS_MODEL, 0, 4, 0)), None);
        assert_eq!(node_cause(node(AttrKind::TLS_MODEL, 0, 5, 0)), un("AttrNode/TLS_MODEL.a"));
        assert_eq!(node_cause(node(AttrKind::CODE_MODEL, 0, 3, 0)), un("AttrNode/CODE_MODEL.a"));
        let zcur = u32::try_from(ZERO_CALL_USED_REGS_KINDS.len()).expect("few");
        let z = AttrKind::ZERO_CALL_USED_REGS;
        assert_eq!(node_cause(node(z, 0, zcur - 1, 0)), None);
        assert_eq!(node_cause(node(z, 0, zcur, 0)), un("AttrNode/ZERO_CALL_USED_REGS.a"));
        let pcs = u32::try_from(CLANG_SECTION_KINDS.len()).expect("few");
        let p = AttrKind::PRAGMA_CLANG_SECTION;
        assert_eq!(node_cause(node(p, 0, 0, pcs)), un("AttrNode/PRAGMA_CLANG_SECTION.b"));
        assert_eq!(node_cause(node(AttrKind::ARM_NEW, 0, 4, 0)), un("AttrNode/ARM_NEW.a"));
        assert_eq!(node_cause(node(AttrKind::ARM_NEW, 0, 3, 7)), None);
        let ps = AttrKind::PASS_OBJECT_SIZE;
        assert_eq!(node_cause(node(ps, 0, 4, 0)), un("AttrNode/PASS_OBJECT_SIZE.a"));
        assert_eq!(node_cause(node(ps, 0, 3, 2)), un("AttrNode/PASS_OBJECT_SIZE.b"));
        let sw = AttrKind::SWIFT_PARAM_ABI;
        assert_eq!(node_cause(node(sw, 0, 4, 0)), None);
        assert_eq!(node_cause(node(sw, 0, 5, 0)), un("AttrNode/SWIFT_PARAM_ABI.a"));
        assert_eq!(node_cause(node(sw, 0, 256, 0)), un("AttrNode/SWIFT_PARAM_ABI.a"));
        let lh = AttrKind::LOOP_HINT;
        assert_eq!(node_cause(node(lh, 0, 0, u32::MAX)), un("AttrNode/LOOP_HINT.b"));
        let g = AttrKind::GSL_OWNER;
        assert_eq!(node_cause(node(g, AttrNode::GSL_HAS_TYPE, 0, 0)), un("AttrNode/GSL_OWNER.b"));
        assert_eq!(node_cause(node(g, 0, 0, 1)), un("AttrNode/GSL_OWNER.b"));
        // Ids that name nothing.
        let big = u32::MAX >> 1;
        assert_eq!(node_cause(node(AttrKind::ALIGNED, 0, big, 0)), un("AttrNode/ALIGNED.a"));
        assert_eq!(node_cause(node(AttrKind::ASSUME, 0, 0, 0)), un("AttrNode/ASSUME.a"));
        assert_eq!(node_cause(node(AttrKind::NONNULL, 0, big, 0)), un("AttrNode/NONNULL.a"));
        assert_eq!(node_cause(node(AttrKind::NONNULL, 0, 0, 0)), un("AttrNode/NONNULL.a"));
        assert_eq!(node_cause(node(AttrKind::ANNOTATE, 0, 0, 0)), None);
        assert_eq!(node_cause(node(AttrKind::ANNOTATE, 0, 0, big)), un("AttrNode/ANNOTATE.b"));
        assert_eq!(node_cause(node(AttrKind::CALLBACK, 0, big, 0)), un("AttrNode/CALLBACK.a"));
        assert_eq!(node_cause(node(AttrKind::ASM_LABEL, 0, 0, 0)), un("AttrNode/ASM_LABEL.a"));
        assert_eq!(node_cause(node(AttrKind::SECTION, 0, big, 0)), un("AttrNode/SECTION.a"));
        assert_eq!(
            node_cause(node(AttrKind::PREFERRED_NAME, 0, 0, 0)),
            un("AttrNode/PREFERRED_NAME.b")
        );
        assert_eq!(node_cause(node(AttrKind::ABI_TAG, 0, big, 0)), un("AttrNode/ABI_TAG.a"));
        // `NO_SANITIZE`: a spelling Kiln does not have.
        let ns = AttrKind::NO_SANITIZE;
        let spellings = u32::try_from(NO_SANITIZE_SPELLINGS.len()).expect("few");
        assert_eq!(node_cause(node(ns, 0, 1, (spellings - 1) << 28)), None);
        assert_eq!(
            node_cause(node(ns, 0, 1, spellings << 28)),
            un("AttrNode/NO_SANITIZE.b-spelling")
        );
        // `unsupported` rows.
        assert_eq!(
            node_cause(node(AttrKind::ANNOTATION, 0, 0, 0)),
            Some(Cause::Unsupported("AttrKind/ANNOTATION"))
        );
        // `CLEANUP.a`, checked: a `DeclId` the arena has.
        assert_eq!(node_cause(node(AttrKind::CLEANUP, 0, big, 0)), un("AttrNode/CLEANUP.a"));
        assert_eq!(node_cause(node(AttrKind::CLEANUP, 0, 0, 0)), un("AttrNode/CLEANUP.a"));
    }

    // §10.1 `NO_SANITIZE.a`, `SanitizerSet` `0`: the names of the set,
    // sorted by their encoding (not in bit order), whichever word holds
    // the bit;
    // `NO_SANITIZE.b-coverage` after them.
    #[test]
    fn s10_1_sanitizer_set() {
        let set = |a: u32, b: u32| {
            with_walk("", "c++20", |w| {
                let mut s = Stream::new();
                attr_node(w, &mut s, &node(AttrKind::NO_SANITIZE, 0, a, b));
                s.as_bytes().to_vec()
            })
        };
        let all = SanitizerSet::ALL;
        let low = SanitizerSet(all.0 & 0xffff_ffff);
        let high = SanitizerSet(all.0 & !0xffff_ffff);
        assert!(high.0 != 0, "Kiln has sanitizers in `b`");
        // Sorted by their encoded bytes (§1.3 `*sorted`): the length first.
        let enc = |n: &str| {
            bytes(|s| {
                s.bytes(n.as_bytes());
            })
        };
        let mut names: Vec<&str> = all.names().collect();
        names.sort_unstable_by_key(|n| enc(n));
        assert_ne!(names.first(), all.names().min().as_ref(), "the order is not the names' order");
        let want = bytes(|s| {
            head(s, attr_kind::NO_SANITIZE, GNU, &[false]);
            s.count(names.len());
            for n in &names {
                s.bytes(n.as_bytes());
            }
            s.bool(true);
        });
        let (a, b) = (low.0 as u32, (high.0 >> 32) as u32);
        assert_eq!(set(a, b | NO_SANITIZE_B_COVERAGE), want);
        // Two members, in the order of their names.
        let first = SanitizerSet(1);
        let second = SanitizerSet(2);
        let (n1, n2) =
            (first.names().next().expect("a name"), second.names().next().expect("a name"));
        let (lo, hi) = if enc(n1) < enc(n2) { (n1, n2) } else { (n2, n1) };
        let want = bytes(|s| {
            head(s, attr_kind::NO_SANITIZE, GNU, &[false]);
            s.count(2).bytes(lo.as_bytes()).bytes(hi.as_bytes()).bool(false);
        });
        assert_eq!(set(3, 0), want);
    }

    /// The parameters named `name`, in the order Kiln made them.
    fn params_named(w: &Walk<'_>, name: &str) -> Vec<DeclId> {
        let ast = w.ast;
        (1..=u32::try_from(ast.decls.len()).expect("few declarations"))
            .map(DeclId::new)
            .filter(|&d| {
                ast.decls.param(d).is_some()
                    && ast
                        .decls
                        .get(d)
                        .name()
                        .as_ident()
                        .is_some_and(|s| ast.symbols().as_str(s) == name)
            })
            .collect()
    }

    /// What `decl_attrs` writes for parameter `name` number `n` of `src`.
    fn param_bytes(src: &str, std: &str, name: &str, n: usize) -> (Vec<u8>, Option<Cause>) {
        with_walk(src, std, |w| {
            let p = params_named(w, name)[n];
            let mut s = Stream::new();
            decl_attrs(w, &mut s, p);
            (s.as_bytes().to_vec(), s.cause())
        })
    }

    // §10.1 the encoding `attrs`, for a parameter (WP-100): `opt` and the
    // definition's own list, then the count of the other declarations'
    // non-empty lists and each of them, first declaration first, then the
    // annotations; read from any of the declarations, the same bytes.
    // `maybe_undef`, which no redeclaration inherits: since Kiln WP-2476 a
    // differing `noescape` is conflicting types, as in clang.
    #[test]
    fn s10_1_param_chain_order() {
        let src = "void f(int *p __attribute__((maybe_undef)));\n\
                   void f(int *p);\n\
                   void f(int *p __attribute__((unused)) __attribute__((unused)));\n\
                   void f(int *p __attribute__((maybe_undef))) {}";
        let want = bytes(|s| {
            s.opt(true).count(1);
            head(s, attr_kind::MAYBE_UNDEF, GNU, &[false]);
            s.count(2).count(1);
            head(s, attr_kind::MAYBE_UNDEF, GNU, &[false]);
            s.count(2);
            head(s, attr_kind::UNUSED, GNU, &[false]);
            head(s, attr_kind::UNUSED, GNU, &[false]);
            no_annotations_of_param(s);
        });
        for n in 0..4 {
            assert_eq!(param_bytes(src, "c++20", "p", n), (want.clone(), None), "parameter {n}");
        }
        // No definition: every list is among the others.
        let src =
            "void f(int *p __attribute__((unused)));\nvoid f(int *p __attribute__((maybe_undef)));";
        let want = bytes(|s| {
            s.opt(false).count(2).count(1);
            head(s, attr_kind::UNUSED, GNU, &[false]);
            s.count(1);
            head(s, attr_kind::MAYBE_UNDEF, GNU, &[false]);
            no_annotations_of_param(s);
        });
        assert_eq!(param_bytes(src, "c++20", "p", 1), (want, None));
    }

    /// A parameter's lists after its attributes: no annotation, no base.
    fn no_annotations_of_param(s: &mut Stream) {
        s.count(0).count(0);
    }

    // §10.1 (WP-100): an entity's lists are kept per declaration: those
    // before the definition, empty lists left out, then the definition's,
    // then those after it.
    #[test]
    fn s10_1_lists_per_declaration() {
        let src = "[[clang::annotate(\"a\")]] void f();\n\
                   void f();\n\
                   __attribute__((noinline)) void f();\n\
                   [[clang::annotate(\"a\")]] __attribute__((cold)) void f() {}\n\
                   __attribute__((noinline)) void f();";
        let ((got, _), ops) = decl_ops(src, "c++20", "f");
        let want = bytes(|s| {
            s.count(2).count(1);
            head(s, attr_kind::ANNOTATE, CXX11, &[false]);
            s.opt(true).raw(&ops[0]).count(0);
            s.count(1);
            head(s, attr_kind::NOINLINE, GNU, &[false]);
            s.opt(true).count(2);
            head(s, attr_kind::ANNOTATE, CXX11, &[false]);
            s.opt(true).raw(&ops[1]).count(0);
            head(s, attr_kind::COLD, GNU, &[false]);
            s.count(1).count(1);
            head(s, attr_kind::NOINLINE, GNU, &[false]);
            s.count(0).count(0);
        });
        assert_eq!(got, want);
    }

    // §10.5 (WP-100): a parameter's annotations are one list over its
    // parameters i, first declaration first, as Kiln's `annotations_of`
    // reads them.
    #[test]
    fn s10_5_param_annotations_over_the_chain() {
        let src = "void f([[=1]] int x);\nvoid f(int x) {}";
        let got = param_bytes(src, "c++26", "x", 1);
        assert_eq!(got, param_bytes("void f(int x);\nvoid f([[=1]] int x) {}", "c++26", "x", 1));
        assert_ne!(got.0, param_bytes("void f(int x);\nvoid f(int x) {}", "c++26", "x", 1).0);
        let two = param_bytes("void f([[=1]] int x);\nvoid f([[=2]] int x) {}", "c++26", "x", 1);
        let swapped =
            param_bytes("void f([[=2]] int x);\nvoid f([[=1]] int x) {}", "c++26", "x", 1);
        assert_ne!(two.0, swapped.0, "first declaration first");
    }

    // §10.1 (WP-100): a block-scope declaration's parameter is left out
    // of the chain of the entity's parameter; the block writes its own
    // (`local::tests::a_block_scope_parameter_writes_its_attributes`).
    #[test]
    fn s10_1_param_chain_leaves_out_block_scope() {
        let src = "void f(int *p);\nvoid g() { void f(int *p __attribute__((nonnull))); }";
        let got = param_bytes(src, "c++20", "p", 0);
        assert_eq!(got, param_bytes("void f(int *p);", "c++20", "p", 0));
        let after = "void g() { void f(int *p __attribute__((nonnull))); }\nvoid f(int *p);";
        assert_eq!(param_bytes(after, "c++20", "p", 1), got);
    }

    // §10.1 last paragraph (WP-100 decision 4): an instantiated
    // declaration's attributes are written as found. Since Kiln WP-2476 an
    // instantiated function's parameter has the attributes of the pattern's
    // parameter (kiln_sema templ/subst/decls.rs `param_infos`), inherited
    // ones included: the explicit instantiation in
    // `differ/_/param-attr-template-first-declaration` changes with them.
    // An attribute written on the definition's parameter alone is on the
    // instantiation's too since Kiln WP-3407, as clang 19 has it (`For
    // Kiln` in `spec/landed/digest-kinds/WP-066.md`), and
    // `differ/_/param-attr-template-definition` changes f<int>.
    #[test]
    fn kiln_instantiates_parameter_attributes() {
        let count = |src: &str| {
            let mut got = Vec::new();
            with_walk(src, "c++20", |w| {
                let ast = w.ast;
                let ps = params_named(w, "p");
                assert_eq!(ps.len(), 3, "two patterns' and the instantiation's");
                got = ps
                    .iter()
                    .map(|&p| ast.lists.iter::<AttrId>(ast.attrs.of_decl(p)).count())
                    .collect();
            });
            got
        };
        let first = count(
            "template<class T> void f(T *p __attribute__((nonnull)));\n\
             template<class T> void f(T *p) {}\n\
             template void f<int>(int *);",
        );
        assert_eq!(first, [1, 1, 1], "inherited by the definition, instantiated from it");
        let definition = count(
            "template<class T> void f(T *p);\n\
             template<class T> void f(T *p __attribute__((nonnull))) {}\n\
             template void f<int>(int *);",
        );
        // Since Kiln WP-3407 (`14b25bd70`, in the pin since WP-163) the
        // definition's attribute is instantiated onto f<int>, as clang 19
        // does (WP-066's For Kiln item, fixed).
        assert_eq!(definition, [0, 1, 1], "the definition's attribute is instantiated");
    }
}
