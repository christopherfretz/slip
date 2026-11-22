//! Constant values and strings (`spec/digest.md` §7,
//! `spec/digest-kinds.md` §8, §8.1, §10.3).
//!
//! A constant is read only through `ConstStore::get` (the decoded
//! `ConstValue`) and written in place by structure: the tag of its
//! `ValueKind`, then the `in` rows of its `Scalar` variant or of its
//! `ByteImage`. Nothing written depends on a `ConstValueId` or on the
//! order Kiln interned the values in; the stored words of §8.1 (`Node`,
//! `AggRec`, `Pools`, `ConstStore`) are all `out`.
//!
//! A value can hold a pointer to another value of the store
//! (`PtrBase::Const`, every `Reloc`), which is written in place by the
//! same rule. What bounds that recursion: the store is append-only and a
//! value can only name values interned before it (`ConstStore::push_scalar`
//! rejects a `PtrBase::Const` id not yet in the store, and `push_aggregate`
//! a relocation to a value that is not pointer-like), so the nesting is a
//! finite DAG; each level enters one level of `walk::DEPTH_LIMIT` (lib.rs
//! "What bounds the stack"), and one top-level value writes at most
//! [`VALUE_BUDGET`] units of work in place (a DAG that shares a subobject
//! is written once per path; `spec/landed/digest-kinds/WP-028.md`).
//!
//! "Padding zeroed" (digest §7): Kiln's image already has it.
//! `ConstStore::push_aggregate` (`kiln_ast/src/consts/mod.rs`, through
//! `Pools::check_image`, lines 638 to 692 at the pinned Kiln) rejects an
//! image with a non-zero byte under a padding range ("non-zero padding
//! byte") or under a relocation ("non-zero bytes under a relocation"), and
//! a store restored from a snapshot is checked the same way
//! (`consts/snap.rs:102`), so `ByteImage::bytes` is
//! written as stored and the ranges that hold no value are written beside
//! it by the rows (`padding`, `relocs`); the shadow runs
//! (`ConstStore::shadow`) are computed from those and are not read.

use slip_kiln::kiln::ast::consts::{
    ByteImage, ByteRange, ConstValue, PtrBase, Reloc, Scalar, UnionMember,
};
use slip_kiln::kiln::ast::decl::LetData;
use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::{ConstValueId, DeclId, ExprId, StringLitId};

use crate::Cause;
use crate::refs::REF_NONE;
use crate::stream::Stream;
use crate::tables::{
    Decision, Row, float_kind, member_kind, null_kind, ptr_base, string_encoding, value_kind,
};
use crate::walk::{TOO_DEEP, Walk};

/// The work one top-level [`value`] may do in place: one unit per value
/// written and one per image byte. Past it the stream is indeterminate
/// with [`TOO_LARGE`]. 2^26 is far above any constant a program holds
/// (a 64 MiB image), and it keeps the work of one value linear in a
/// bound even when its pointers share a subobject many times.
const VALUE_BUDGET: u64 = 1 << 26;

/// The cause of a value larger than [`VALUE_BUDGET`].
const TOO_LARGE: Cause = Cause::Unaccounted("value/size");

/// What a value that cannot be written puts in its place: `uv 0`, which
/// no tag is (digest-kinds §1.4), as the other areas do.
const PLACEHOLDER: u64 = 0;

/// `value`: the constant `v` by the rows of §8.
pub(crate) fn value(w: &mut Walk<'_>, s: &mut Stream, v: ConstValueId) {
    let mut budget = VALUE_BUDGET;
    write_value(w, s, v, &mut budget);
}

/// `string`: a string literal by content, the rows of its `Entry`
/// (§10.3): `start` as the code-unit bytes Kiln stores, then `enc`.
pub(crate) fn string(w: &mut Walk<'_>, s: &mut Stream, l: StringLitId) {
    let (enc, bytes) = w.ast.strings.get(l);
    s.bytes(bytes).tag(string_encoding::row(&enc));
}

/// Takes `n` units of the budget; `false` (and the stream is
/// indeterminate) once it is spent.
fn spend(s: &mut Stream, budget: &mut u64, n: u64) -> bool {
    if *budget < n {
        *budget = 0;
        s.indeterminate(TOO_LARGE);
        return false;
    }
    *budget -= n;
    true
}

/// The tag of a variant row, and the row's cause if it is `unsupported`
/// (digest-kinds §1.2).
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
fn write_value(w: &mut Walk<'_>, s: &mut Stream, v: ConstValueId, budget: &mut u64) {
    if !spend(s, budget, 1) {
        s.uv(PLACEHOLDER);
        return;
    }
    if !w.enter() {
        s.uv(PLACEHOLDER).indeterminate(TOO_DEEP);
        return;
    }
    let ast = w.ast;
    match ast.consts.get(v) {
        ConstValue::Scalar(x) => write_scalar(w, s, x, budget),
        ConstValue::Aggregate(img) => write_image(w, s, img, budget),
    }
    w.leave();
}

/// The rows of the `Scalar` variant, after its `ValueKind` tag.
fn write_scalar(w: &mut Walk<'_>, s: &mut Stream, x: Scalar<'_>, budget: &mut u64) {
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
            variant(s, float_kind::row(&kind), "FloatKind");
            // `Float.bits`: the exact encoding, 16 bytes little-endian.
            s.bytes(&bits.to_le_bytes());
        }
        Scalar::Null { ty, kind } => {
            variant(s, value_kind::NULL, "ValueKind/Null");
            w.write_type(s, ty);
            variant(s, null_kind::row(&kind), "NullKind");
        }
        Scalar::Pointer { ty, base, offset, designated, past } => {
            variant(s, value_kind::POINTER, "ValueKind/Pointer");
            w.write_type(s, ty);
            write_base(w, s, base, budget);
            s.uv(offset);
            w.write_type_opt(s, designated);
            s.bool(past);
        }
        Scalar::MemberPtr { ty, member, kind, derived, path } => {
            variant(s, value_kind::MEMBER_PTR, "ValueKind/MemberPtr");
            w.write_type(s, ty);
            w.write_ref(s, member);
            variant(s, member_kind::row(&kind), "MemberKind");
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

/// `enum PtrBase`.
fn write_base(w: &mut Walk<'_>, s: &mut Stream, base: PtrBase, budget: &mut u64) {
    let row = ptr_base::row(&base);
    variant(s, row, "PtrBase");
    match base {
        PtrBase::Decl(d) | PtrBase::Function(d) => w.write_ref(s, d),
        PtrBase::Temporary(d) => write_temporary(w, s, d),
        PtrBase::StringLit { lit, origin: _ } => string(w, s, lit),
        PtrBase::Const(c) => write_value(w, s, c, budget),
        PtrBase::TypeInfo(q) => w.write_type(s, q),
        PtrBase::Label { label, origin: _ } => w.write_ref(s, label),
        PtrBase::Absolute => {}
        PtrBase::LabelDiff { plus, minus, size } => {
            write_addr_label(w, s, plus, "PtrBase/LabelDiff.plus");
            write_addr_label(w, s, minus, "PtrBase/LabelDiff.minus");
            s.uv(u64::from(size));
        }
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
fn write_image(w: &mut Walk<'_>, s: &mut Stream, img: ByteImage<'_>, budget: &mut u64) {
    let ByteImage { ty, bytes, relocs, padding, unions } = img;
    variant(s, value_kind::AGGREGATE, "ValueKind/Aggregate");
    w.write_type(s, ty);
    // `bytes`: as stored; Kiln zeroes padding and relocated bytes (the
    // module documentation says where).
    if spend(s, budget, bytes.len() as u64) {
        s.bytes(bytes);
    } else {
        s.bytes(&[]);
    }
    s.count(relocs.len());
    for &Reloc { at, value } in relocs {
        s.uv(u64::from(at));
        write_value(w, s, value, budget);
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

    use std::sync::Arc;

    use slip_kiln::kiln::ast::consts::{ConstValue, IntBits, PtrBase, Scalar};
    use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, DeclMisc, NewDecl, Payload};
    use slip_kiln::kiln::ast::names::DeclName;
    use slip_kiln::kiln::ast::strings::StringEncoding;
    use slip_kiln::kiln::ast::types::TypeContext;
    use slip_kiln::kiln::ast::{Ast, ConstValueId, QualType};
    use slip_kiln::kiln::source::Loc;
    use slip_kiln::kiln::source::SourceManager;
    use slip_kiln::kiln::source::symbol::Interner;
    use slip_model::keys::RuleTable;

    use super::{TOO_LARGE, value, write_value};
    use crate::refs::REF_LOCAL;
    use crate::stream::Stream;
    use crate::tables::{float_kind, ptr_base, string_encoding, value_kind};
    use crate::walk::{TOO_DEEP, Walk};
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

    fn label(ast: &mut Ast) -> slip_kiln::kiln::ast::DeclId {
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
            let mut want = Stream::new();
            want.tag(value_kind::POINTER);
            w.write_type(&mut want, QualType::LONG);
            want.tag(ptr_base::LABEL_DIFF).uv(REF_LOCAL).uv(0).uv(REF_LOCAL).uv(1).uv(8);
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

    // lib.rs "What bounds the stack": a chain of pointers to constants
    // deeper than the walk's limit is `walk/depth`, not a stack overflow.
    #[test]
    fn nesting_is_bounded() {
        let (mut ast, ty) = hand_built();
        let mut v = ast.consts.push_scalar(Scalar::Int {
            ty: QualType::INT,
            signed: true,
            bits: IntBits::from_i128(32, 7),
        });
        for _ in 0..300 {
            let base = PtrBase::Const(v.expect("a value"));
            v = ast.consts.push_scalar(Scalar::Pointer {
                ty,
                base,
                offset: 0,
                designated: None,
                past: false,
            });
        }
        let v = v.expect("a value");
        with_ast(&ast, |w| {
            let got = written(w, v);
            assert_eq!(got.cause(), Some(TOO_DEEP));
            assert_eq!(w.depth, 0, "every level entered is left");
        });
    }

    // WP-028 (`spec/landed/digest-kinds/WP-028.md`): one value does at most
    // its budget of work in place; past it, `value/size`.
    #[test]
    fn work_is_bounded() {
        let src = "template <auto V> struct A;\nint g;\nstruct P { const int* p; int a[8]; };\nA<P{&g}>* x;\n";
        with_walk(src, "c++20", |w| {
            let v = find(w, |c| matches!(c, ConstValue::Aggregate(_)))[0];
            let mut s = Stream::new();
            let mut budget = 1 + 40 + 1;
            write_value(w, &mut s, v, &mut budget);
            assert_eq!(s.cause(), None, "the image and its one pointer fit");
            assert_eq!(budget, 0);
            let mut s = Stream::new();
            let mut budget = 1 + 40;
            write_value(w, &mut s, v, &mut budget);
            assert_eq!(s.cause(), Some(TOO_LARGE));
            let mut s = Stream::new();
            let mut budget = 1 + 39;
            write_value(w, &mut s, v, &mut budget);
            assert_eq!(s.cause(), Some(TOO_LARGE));
        });
    }
}
