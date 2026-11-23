//! Row-level tests of the core kinds (WP-031 review item 6): a pair test
//! cannot isolate a row whose value follows from types the pair also
//! changes (a `CastKind`, a `CharKind`), so these tests read the rows the
//! walk writes for one node and compare them with rows written by hand
//! from `crate::tables`.
//!
//! The sources are the pair tests' own: for each directory of a table
//! (`tests/differ/<Table>/<row>/`) both sides are parsed, every node of
//! the kinds that write the row is read, and one of them must write the
//! row the directory names. So every reachable row of an enumeration and
//! every flag row of the core kinds has a test that fails when the walk
//! writes a constant in its place or drops it.
//!
//! The rows of a node are rendered one string per primitive: `child` for
//! a child (not followed), `tag N`, `flags N` (the word's value), `type T`
//! (Kiln's canonical spelling), `bases N` (the length of a base path),
//! and so on (`render`).

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use slip_kiln::kiln::ast::expr::{
    BinaryOp, CastKind, CharKind, ExprDeps, ExprKind, NonOdrUse, StorageDuration, StringKind,
    UnaryOp,
};
use slip_kiln::kiln::ast::fp::FpRound;
use slip_kiln::kiln::ast::names::OverloadedOperator;
use slip_kiln::kiln::ast::types::{PrintOptions, print_type};
use slip_kiln::kiln::ast::{ExprId, QualType};
use slip_model::keys::RuleTable;

use super::{Item, cold, dispatch, node, opaque, write_node};
use crate::Input;
use crate::stream::{FlagWord, Stream};
use crate::tables::{
    Row, binary_op, cast_kind, char_kind, expr_kind, fp_round, non_odr_use, object_kind,
    overloaded_operator, storage_duration, string_kind, unary_op, value_category,
};
use crate::walk::{ExprState, Walk};

/// `tests/<root>/<table>/`.
fn table_dir(root: &str, table: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(root).join(table)
}

/// The test directories of `table` under `tests/differ/`, by row name.
fn rows_of(table: &str) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(table_dir("differ", table))
        .expect("the table's tests")
        .map(|d| {
            let d = d.expect("an entry").path();
            (d.file_name().expect("a name").to_string_lossy().into_owned(), d)
        })
        .collect();
    out.sort();
    out
}

/// Runs `f` on a walk over each side of the pair test in `dir`, as
/// `tests/common` parses it (`side.h`, `// std:`); errors are allowed.
fn each_side(dir: &Path, mut f: impl FnMut(&mut Walk<'_>)) {
    let mut all: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("the test")
        .map(|d| {
            let p = d.expect("an entry").path();
            let name = p.file_name().expect("a name").to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).expect("a text file"))
        })
        .collect();
    all.sort();
    let get = |n: &str| all.iter().find(|(m, _)| m == n).map(|(_, t)| t.clone());
    for b in [false, true] {
        let mut list: Vec<(String, String)> = Vec::new();
        let main = if let Some(t) = get("t.cpp") {
            list.push(("side.h".into(), if b { "#define B 1\n".into() } else { String::new() }));
            t
        } else {
            get(if b { "b.cpp" } else { "a.cpp" }).expect("t.cpp, or a.cpp and b.cpp")
        };
        list.push(("main.cpp".into(), main.clone()));
        for (n, t) in &all {
            if !matches!(n.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
                list.push((n.clone(), t.clone()));
            }
        }
        let std = main
            .lines()
            .take(4)
            .find_map(|l| l.strip_prefix("// std:"))
            .map_or("c++20", str::trim)
            .to_owned();
        let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let snippet = slip_kiln::snippet::Snippet {
            files: &refs,
            main: "main.cpp",
            include: &[""],
            std: &std,
        };
        slip_kiln::snippet::parse(&snippet, |p| {
            let path = |_| Vec::new();
            let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
            let rules = RuleTable::empty();
            let mut w = Walk::new(&input, &rules);
            f(&mut w);
        })
        .expect("parses");
    }
}

/// Every expression of the walk's AST.
fn exprs(w: &Walk<'_>) -> Vec<ExprId> {
    (1..=w.ast.exprs.len()).filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?)).collect()
}

fn ty(w: &Walk<'_>, q: QualType) -> String {
    format!("type {}", print_type(&w.ast.types, w.ast, q, PrintOptions::CANONICAL))
}

/// One string per primitive of `items` (module documentation).
fn render(w: &Walk<'_>, items: &[Item]) -> Vec<String> {
    items
        .iter()
        .map(|i| match *i {
            Item::Node(..) | Item::Decayed(_) => "child".to_owned(),
            Item::Uv(v) => format!("uv {v}"),
            Item::Bool(v) => format!("bool {v}"),
            Item::Tag(r) => format!("tag {}", r.tag),
            Item::Flags(f) => format!("flags {}", f.value()),
            Item::Count(n) => format!("count {n}"),
            Item::Opt(p) => format!("opt {p}"),
            Item::Ref(_) | Item::RefOpt(Some(_)) => "ref".to_owned(),
            Item::RefOpt(None) => "ref none".to_owned(),
            Item::Type(q) | Item::TypeOpt(Some(q)) => ty(w, q),
            Item::TypeOpt(None) => "type none".to_owned(),
            Item::BasePath(l) => format!("bases {}", l.map_or(0, |l| w.ast.lists.get(l).len())),
            Item::Value(_) => "value".to_owned(),
            Item::String(_) => "string".to_owned(),
            Item::Args(_) => "args".to_owned(),
            Item::Decl(_) => "decl".to_owned(),
            Item::Cause(c) => format!("cause {c}"),
            Item::Opaque(_) => "opaque".to_owned(),
            Item::OpaqueSource(_) => "opaque-source".to_owned(),
            Item::Exec(..) => "exec".to_owned(),
            Item::LeaveDependent => "leave".to_owned(),
        })
        .collect()
}

/// The rows of `e`'s kind, as the walk writes them inside an expression
/// as dependent as `e` itself.
fn kind_rows(w: &mut Walk<'_>, e: ExprId) -> Vec<String> {
    let n = node(w, e);
    let dependent = w.ast.exprs.get(e).deps().contains(ExprDeps::INSTANTIATION);
    w.unit.expr.dependent = u32::from(dependent);
    let mut s = Stream::new();
    let mut out = Vec::new();
    dispatch(w, &mut s, e, &n, &mut out);
    w.unit.expr = ExprState::default();
    render(w, &out)
}

/// The rows of `ColdExprData` the walk writes for `e`.
fn cold_rows(w: &mut Walk<'_>, e: ExprId) -> Vec<String> {
    let n = node(w, e);
    let mut s = Stream::new();
    let mut scratch = Vec::new();
    let tail = dispatch(w, &mut s, e, &n, &mut scratch);
    let mut out = Vec::new();
    cold(w, e, &n, tail, &mut out);
    w.unit.expr = ExprState::default();
    render(w, &out)
}

/// The bytes of the common rows the walk writes for `e` (§7.1: the kind,
/// `bits-value-category`, `bits-object-kind`, `deps`, `ty`).
fn common_bytes(w: &mut Walk<'_>, e: ExprId) -> Vec<u8> {
    let mut s = Stream::new();
    let mut items = Vec::new();
    write_node(w, &mut s, e, true, false, &mut items);
    w.unit.expr = ExprState::default();
    s.as_bytes().to_vec()
}

/// The variant of a Kiln `u8` enumeration whose name is `name`.
fn by_name<V: Debug>(from: fn(u8) -> Option<V>, name: &str) -> Option<V> {
    (0..=u8::MAX).filter_map(from).find(|v| format!("{v:?}") == name)
}

/// For each test directory of `table` (pending ones too: the nodes exist
/// even where the digest of what holds them is pending): some node of
/// `kinds` on one side writes, as its first `tag`, the row `row` gives
/// for the directory's name. Directories in `skip` are rows Kiln never
/// builds or another package writes.
fn first_tag_rows(
    table: &str,
    kinds: &[ExprKind],
    row: &dyn Fn(&str) -> Option<Row>,
    skip: &[&str],
) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, dir) in rows_of(table) {
        if skip.contains(&name.as_str()) {
            continue;
        }
        let want =
            format!("tag {}", row(&name).unwrap_or_else(|| panic!("{table}/{name}: no row")).tag);
        let mut seen = Vec::new();
        each_side(&dir, |w| {
            for e in exprs(w) {
                if kinds.contains(&w.ast.exprs.get(e).kind()) {
                    let rows = kind_rows(w, e);
                    seen.push(rows.into_iter().find(|r| r.starts_with("tag ")).unwrap_or_default());
                }
            }
        });
        if !seen.contains(&want) {
            failures.push(format!("{table}/{name}: no node writes `{want}`; seen {seen:?}"));
        }
    }
    failures
}

fn check(failures: &[String]) {
    assert!(failures.is_empty(), "{} row(s) not written:\n{}", failures.len(), failures.join("\n"));
}

const CASTS: &[ExprKind] = &[
    ExprKind::ImplicitCastExpr,
    ExprKind::CStyleCastExpr,
    ExprKind::CXXFunctionalCastExpr,
    ExprKind::CXXStaticCastExpr,
    ExprKind::CXXDynamicCastExpr,
    ExprKind::CXXConstCastExpr,
    ExprKind::CXXReinterpretCastExpr,
    ExprKind::BuiltinBitCastExpr,
];

// §7.2 `CastKind`: every row but `ToUnion`, which Kiln never builds
// (kiln_ast expr.rs:624; matched only at kiln_sema static_tls.rs:163 and
// class/temporary/lifetime.rs:717).
#[test]
fn every_cast_kind_is_written() {
    let row = |n: &str| by_name(CastKind::from_u8, n).map(|v| cast_kind::row(&v));
    check(&first_tag_rows("CastKind", CASTS, &row, &[]));
}

// §7.2 `UnaryOp`, every row but `Coawait`: Kiln builds a `co_await` as a
// `CoawaitExpr` and never a built-in unary operator of it (kiln_sema
// expr.rs:3667, unreachable).
#[test]
fn every_unary_op_is_written() {
    let row = |n: &str| by_name(UnaryOp::from_u8, n).map(|v| unary_op::row(&v));
    check(&first_tag_rows("UnaryOp", &[ExprKind::UnaryOperator], &row, &["Coawait"]));
}

// §7.2 `BinaryOp`, every row, of a `BinaryOperator` or a
// `CompoundAssignOperator`.
#[test]
fn every_binary_op_is_written() {
    let row = |n: &str| by_name(BinaryOp::from_u8, n).map(|v| binary_op::row(&v));
    let kinds = [ExprKind::BinaryOperator, ExprKind::CompoundAssignOperator];
    check(&first_tag_rows("BinaryOp", &kinds, &row, &[]));
}

// §7.2 `CharKind` and `StringKind`, every row.
#[test]
fn every_char_and_string_kind_is_written() {
    let row = |n: &str| by_name(CharKind::from_u8, n).map(|v| char_kind::row(&v));
    let mut f = first_tag_rows("CharKind", &[ExprKind::CharacterLiteral], &row, &[]);
    let row = |n: &str| by_name(StringKind::from_u8, n).map(|v| string_kind::row(&v));
    f.extend(first_tag_rows("StringKind", &[ExprKind::StringLiteral], &row, &[]));
    check(&f);
}

// §7.2 `NonOdrUse`, every row Kiln builds (`Discarded` is never set: kiln_ast
// expr.rs:720, sema sets only `Constant` and `Unevaluated`), of a
// `DeclRefExpr` or a `MemberExpr`.
#[test]
fn every_non_odr_use_is_written() {
    let row = |n: &str| by_name(NonOdrUse::from_u8, n).map(|v| non_odr_use::row(&v));
    let kinds = [ExprKind::DeclRefExpr, ExprKind::MemberExpr];
    check(&first_tag_rows("NonOdrUse", &kinds, &row, &[]));
}

// §7.2 `StorageDuration` of a `MaterializeTemporaryExpr`, every row.
#[test]
fn every_storage_duration_is_written() {
    let row = |n: &str| by_name(StorageDuration::from_u8, n).map(|v| storage_duration::row(&v));
    check(&first_tag_rows("StorageDuration", &[ExprKind::MaterializeTemporaryExpr], &row, &[]));
}

/// §7.1 the common rows: for each test directory of `ValueCategory`,
/// `ObjectKind` and `ExprDeps`, some node writes the row by hand: the
/// kind's tag, then the category, the object kind and the dependence word
/// with the directory's row in place of Kiln's value.
#[test]
fn every_common_row_is_written() {
    let mut failures = Vec::new();
    let tables: [(&str, &[(&str, Row)]); 2] = [
        (
            "ValueCategory",
            &[
                ("PRValue", value_category::PR_VALUE),
                ("LValue", value_category::L_VALUE),
                ("XValue", value_category::X_VALUE),
            ],
        ),
        (
            "ObjectKind",
            &[
                ("Ordinary", object_kind::ORDINARY),
                ("BitField", object_kind::BIT_FIELD),
                ("VectorComponent", object_kind::VECTOR_COMPONENT),
            ],
        ),
    ];
    for (table, rows) in tables {
        for (name, dir) in rows_of(table) {
            let row = rows.iter().find(|(n, _)| *n == name).expect("a row").1;
            let mut found = false;
            each_side(&dir, |w| {
                for e in exprs(w) {
                    let words = *w.ast.exprs.get(e);
                    let mut want = Stream::new();
                    want.tag(expr_kind::row(&words.kind()));
                    if table == "ValueCategory" {
                        want.tag(row);
                    } else {
                        want.tag(value_category::row(&words.value_category())).tag(row);
                    }
                    found |= common_bytes(w, e).starts_with(want.as_bytes());
                }
            });
            if !found {
                failures.push(format!("{table}/{name}"));
            }
        }
    }
    let bits = [
        ("0-type", ExprDeps::TYPE),
        ("0-value", ExprDeps::VALUE),
        ("0-instantiation", ExprDeps::INSTANTIATION),
        ("0-unexpanded-pack", ExprDeps::UNEXPANDED_PACK),
    ];
    for (name, dir) in rows_of("ExprDeps") {
        let bit = bits.iter().find(|(n, _)| *n == name).expect("a row").1;
        let mut found = false;
        each_side(&dir, |w| {
            for e in exprs(w) {
                let words = *w.ast.exprs.get(e);
                let d = words.deps();
                if !d.contains(bit) {
                    continue;
                }
                let has = |b: ExprDeps| b == bit || d.contains(b);
                let mut want = Stream::new();
                want.tag(expr_kind::row(&words.kind()))
                    .tag(value_category::row(&words.value_category()))
                    .tag(object_kind::row(&words.object_kind()))
                    .flags(
                        FlagWord::new()
                            .bit(has(ExprDeps::TYPE))
                            .bit(has(ExprDeps::VALUE))
                            .bit(has(ExprDeps::INSTANTIATION))
                            .bit(has(ExprDeps::UNEXPANDED_PACK)),
                    );
                found |= common_bytes(w, e).starts_with(want.as_bytes());
            }
        });
        if !found {
            failures.push(format!("ExprDeps/{name}"));
        }
    }
    check(&failures);
}

/// The flag rows of the core kinds: the test directory of each, the kinds
/// that write it, and the bit's place in the kind's first flag word.
const FLAGS: &[(&str, &[ExprKind], u32)] = &[
    ("IntegerLiteral.bits-wide", &[ExprKind::IntegerLiteral], 0),
    ("DeclRefExpr.bits-names-closure-copy", &[ExprKind::DeclRefExpr], 0),
    ("DeclRefExpr.bits-refers-to-enclosing", &[ExprKind::DeclRefExpr], 1),
    ("DeclRefExpr.bits-immediate-escalating", &[ExprKind::DeclRefExpr], 2),
    ("MemberExpr.bits-arrow", &[ExprKind::MemberExpr], 0),
    ("CXXThisExpr.aux-implicit", &[ExprKind::CXXThisExpr], 0),
    ("UnaryOperator.bits-cannot-overflow", &[ExprKind::UnaryOperator], 0),
    ("CallExpr.bits-assume-ignored", &[ExprKind::CallExpr], 0),
    ("ImplicitCastExpr.bits-part-of-explicit-cast", &[ExprKind::ImplicitCastExpr], 0),
    ("MaterializeTemporaryExpr.aux-lvalue-reference", &[ExprKind::MaterializeTemporaryExpr], 0),
    ("ExprWithCleanups.aux-side-effects", &[ExprKind::ExprWithCleanups], 0),
    ("ConstantExpr.aux-elided", &[ExprKind::ConstantExpr], 0),
    ("ConstantExpr.aux-value-hidden", &[ExprKind::ConstantExpr], 1),
];

// §7 every flag row of the core kinds: some node of its test writes it set.
#[test]
fn every_flag_row_is_written() {
    let mut failures = Vec::new();
    for &(name, kinds, bit) in FLAGS {
        let mut found = false;
        each_side(&table_dir("differ", "ExprNode").join(name), |w| {
            for e in exprs(w) {
                if !kinds.contains(&w.ast.exprs.get(e).kind()) {
                    continue;
                }
                let rows = kind_rows(w, e);
                let word = rows.iter().find_map(|r| r.strip_prefix("flags ")?.parse::<u64>().ok());
                found |= word.is_some_and(|v| v >> bit & 1 == 1);
            }
        });
        if !found {
            failures.push(format!("ExprNode/{name}: no node writes the bit set"));
        }
    }
    check(&failures);
}

/// Some node of `kinds` in the test `dir` (`tests/differ/<dir>`) writes
/// rows (`kind_rows`, or `cold_rows` with `cold`) for which `pred` holds.
fn some_node(
    dir: &str,
    kinds: &[ExprKind],
    cold: bool,
    pred: &dyn Fn(&[String]) -> bool,
) -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differ").join(dir);
    let mut found = false;
    let mut seen = Vec::new();
    each_side(&path, |w| {
        for e in exprs(w) {
            if kinds.is_empty() || kinds.contains(&w.ast.exprs.get(e).kind()) {
                let rows = if cold { cold_rows(w, e) } else { kind_rows(w, e) };
                found |= pred(&rows);
                seen.push(rows);
            }
        }
    });
    (!found).then(|| format!("{dir}: no node; seen {seen:?}"))
}

/// `pat` occurs in `rows` as consecutive entries.
fn has_run(rows: &[String], pat: &[&str]) -> bool {
    rows.windows(pat.len()).any(|w| w.iter().zip(pat).all(|(a, b)| a == b))
}

// The rows the review found no test could fail for, other than enumeration
// tags and flags: base paths, `MaterializeTemporaryExpr.b`,
// `CXXBindTemporaryExpr.b`, `CompoundAssignExtra.lhs_ty`,
// `CXXOperatorCallExpr.aux` and `.a`, `CXXBoolLiteralExpr.a`,
// `ColdExprData` `union_field` and `fp_round`.
#[test]
fn every_other_row_is_written() {
    let mut f = Vec::new();
    // §7 `ImplicitCastExpr.b` and the explicit casts' `.b`: a base path of
    // one step.
    for dir in [
        "ExprNode/ImplicitCastExpr.b",
        "ExprNode/CStyleCastExpr.b",
        "ExprNode/CXXStaticCastExpr.b",
        "ExprNode/CXXFunctionalCastExpr.b",
        "ExprNode/CXXDynamicCastExpr.b",
    ] {
        f.extend(some_node(dir, CASTS, false, &|r| r.last().is_some_and(|b| b == "bases 1")));
    }
    // `MaterializeTemporaryExpr.b`: `decl?` present.
    f.extend(some_node(
        "ExprNode/MaterializeTemporaryExpr.b",
        &[ExprKind::MaterializeTemporaryExpr],
        false,
        &|r| has_run(r, &["child", "opt true", "decl"]),
    ));
    // `CXXBindTemporaryExpr.b`: the destructor.
    f.extend(some_node(
        "ExprNode/CXXBindTemporaryExpr.b",
        &[ExprKind::CXXBindTemporaryExpr],
        false,
        &|r| r == ["child", "ref"],
    ));
    // `CompoundAssignExtra`: `rhs`, `lhs_ty`, `result_ty` after `aux` and
    // `a` (`b += p` for a `bool b`: the left computation type is `int`,
    // the result one `int *`, so the two rows cannot be swapped unseen).
    let add = format!("tag {}", binary_op::ADD_ASSIGN.tag);
    f.extend(some_node(
        "CompoundAssignExtra/lhs_ty",
        &[ExprKind::CompoundAssignOperator],
        false,
        &|r| r.len() == 5 && r[..4] == [add.as_str(), "child", "child", "type int"] && r[4] != r[3],
    ));
    // `CXXOperatorCallExpr`: `aux` the operator, `a` the callee, `b` two
    // arguments.
    for (dir, op) in [
        ("ExprNode/CXXOperatorCallExpr.aux", overloaded_operator::PLUS),
        ("ExprNode/CXXOperatorCallExpr.aux", overloaded_operator::MINUS),
    ] {
        let tag = format!("tag {}", op.tag);
        f.extend(some_node(dir, &[ExprKind::CXXOperatorCallExpr], false, &|r| {
            r == [tag.as_str(), "child", "count 2", "child", "child"]
        }));
    }
    // `CXXBoolLiteralExpr.a`: both values.
    for v in ["bool true", "bool false"] {
        f.extend(some_node(
            "ExprNode/CXXBoolLiteralExpr.a",
            &[ExprKind::CXXBoolLiteralExpr],
            false,
            &|r| r == [v],
        ));
    }
    // `ColdExprData` `union_field`: the only `ref` of the cold rows.
    f.extend(some_node("ColdExprData/union_field", &[], true, &|r| r.iter().any(|x| x == "ref")));
    // `ColdExprData` `fp_round`: the last rows, `FE_UPWARD`.
    let up = format!("tag {}", fp_round::UPWARD.tag);
    f.extend(some_node("ColdExprData/fp_round", &[], true, &|r| {
        r.ends_with(&["opt true".into(), up.clone()])
    }));
    check(&f);
}

// §7 "Opaque values": sources numbered in the order reached; a source
// reached again writes its number and an absent child; a source numbered
// by `OpaqueSource` writes its number alone the first time too.
#[test]
fn opaque_values_are_numbered() {
    let src = "int a, b;\nint v = a ?: b;\nint u = b ?: a;\n";
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse(&snippet, |p| {
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let mut w = Walk::new(&input, &rules);
        let ovs: Vec<ExprId> = exprs(&w)
            .into_iter()
            .filter(|&e| w.ast.exprs.get(e).kind() == ExprKind::OpaqueValueExpr)
            .collect();
        let source = |w: &Walk<'_>, e: ExprId| w.ast.exprs.get(e).a();
        // One opaque value per `?:` (the condition and the true arm share
        // it), one source each.
        assert_eq!(ovs.len(), 2, "{ovs:?}");
        assert_ne!(source(&w, ovs[0]), source(&w, ovs[1]));
        let (first, other) = (ovs[0], ovs[1]);
        let mut got = Stream::new();
        let mut stack = Vec::new();
        opaque(&mut w, &mut got, first, &mut stack);
        opaque(&mut w, &mut got, first, &mut stack);
        opaque(&mut w, &mut got, other, &mut stack);
        let mut want = Stream::new();
        want.uv(0).opt(true).uv(0).opt(false).uv(1).opt(true);
        assert_eq!(got.as_bytes(), want.as_bytes());
        assert_eq!(stack.len(), 2, "each source once");
        // Numbered beforehand: the number alone.
        w.unit.expr = ExprState::default();
        let src = ExprId::try_new(source(&w, other)).expect("a source");
        let mut s = Stream::new();
        super::walk_items(&mut w, &mut s, Item::OpaqueSource(src));
        assert!(s.as_bytes().is_empty(), "numbering writes nothing");
        let mut got = Stream::new();
        let mut stack = Vec::new();
        opaque(&mut w, &mut got, other, &mut stack);
        let mut want = Stream::new();
        want.uv(0).opt(false);
        assert_eq!(got.as_bytes(), want.as_bytes());
        assert!(stack.is_empty());
    })
    .expect("parses");
}

/// Parses `src` alone and runs `f` on a walk of it.
fn with_source(src: &str, std: &str, f: impl FnOnce(&mut Walk<'_>)) {
    let files = [("main.cpp", src)];
    let snippet =
        slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
    slip_kiln::snippet::parse(&snippet, |p| {
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let mut w = Walk::new(&input, &rules);
        f(&mut w);
    })
    .expect("parses");
}

/// The first `tag` each node of `kind` writes (kind rows when `!cold`,
/// cold rows when `cold`), over a walk of `src`.
fn tags_of(src: &str, std: &str, kind: Option<ExprKind>, cold: bool) -> Vec<String> {
    let mut tags = Vec::new();
    with_source(src, std, |w| {
        for e in exprs(w) {
            if kind.is_none_or(|k| w.ast.exprs.get(e).kind() == k) {
                let rows = if cold { cold_rows(w, e) } else { kind_rows(w, e) };
                tags.extend(rows.into_iter().filter(|r| r.starts_with("tag ")));
            }
        }
    });
    tags
}

// §7.2 `CXXOperatorCallExpr.aux` (`OverloadedOperator`, §10.2), every row an
// operator call can carry: each operator of a class called in operator
// form. `New`, `Delete`, `ArrayNew` and `ArrayDelete` are never an operator
// call (a new or delete expression names its function); `Coawait` is the
// call of a non-member `operator co_await` (kiln_sema coroutine.rs:534).
#[test]
fn every_overloaded_operator_of_a_call_is_written() {
    let src = r"
struct S {
  int m;
  S operator+(S) const; S operator-(S) const; S operator*(S) const;
  S operator/(S) const; S operator%(S) const; S operator^(S) const;
  S operator&(S) const; S operator|(S) const; S operator~() const;
  S operator!() const; S &operator=(const S &); S operator<(S) const;
  S operator>(S) const; S &operator+=(S); S &operator-=(S); S &operator*=(S);
  S &operator/=(S); S &operator%=(S); S &operator^=(S); S &operator&=(S);
  S &operator|=(S); S operator<<(S) const; S operator>>(S) const;
  S &operator<<=(S); S &operator>>=(S); S operator==(S) const;
  S operator!=(S) const; S operator<=(S) const; S operator>=(S) const;
  int operator<=>(S) const; S operator&&(S) const; S operator||(S) const;
  S &operator++(); S &operator--(); S operator,(S) const;
  S operator->*(int) const; S *operator->(); S operator()() const;
  S operator[](int) const;
};
S s, t;
void f() {
  s + t; s - t; s * t; s / t; s % t; s ^ t; s & t; s | t; ~s; !s; s = t;
  s < t; s > t; s += t; s -= t; s *= t; s /= t; s %= t; s ^= t; s &= t;
  s |= t; s << t; s >> t; s <<= t; s >>= t; s == t; s != t; s <= t;
  s >= t; s <=> t; s && t; s || t; ++s; --s; s, t; s->*0; s->m; s(); s[0];
}
namespace std {
template <class R, class...> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle {
  static coroutine_handle from_address(void *);
  operator coroutine_handle<>() const;
};
template <> struct coroutine_handle<void> { static coroutine_handle from_address(void *); };
struct suspend_never {
  bool await_ready() const noexcept; void await_suspend(coroutine_handle<>) const noexcept;
  void await_resume() const noexcept;
};
}
struct Aw {};
std::suspend_never operator co_await(Aw);
struct Task {
  struct promise_type {
    Task get_return_object(); std::suspend_never initial_suspend() noexcept;
    std::suspend_never final_suspend() noexcept; void return_void(); void unhandled_exception();
  };
};
Task g() { co_await Aw{}; }
";
    let tags = tags_of(src, "c++20", Some(ExprKind::CXXOperatorCallExpr), false);
    let mut missing = Vec::new();
    for op in (1..=u8::MAX).filter_map(OverloadedOperator::from_u8) {
        use OverloadedOperator as O;
        if matches!(op, O::New | O::Delete | O::ArrayNew | O::ArrayDelete) {
            continue;
        }
        let want = format!("tag {}", overloaded_operator::row(&op).tag);
        if !tags.contains(&want) {
            missing.push(format!("{op:?}"));
        }
    }
    assert!(missing.is_empty(), "not written: {missing:?}; seen {tags:?}");
}

// §6 `FpRound` of `ColdExprData` `fp_round` (§7.4): one function per
// `#pragma STDC FENV_ROUND` mode. `Dynamic` and `Nearest` never enter the
// map: Kiln records only a directed mode (kiln_sema stmt.rs:400,
// `FpRound::is_directed`, kiln_ast fp.rs:50).
#[test]
fn every_fp_round_is_written() {
    let modes = [
        ("FE_UPWARD", FpRound::Upward),
        ("FE_DOWNWARD", FpRound::Downward),
        ("FE_TOWARDZERO", FpRound::TowardZero),
        ("FE_TONEARESTFROMZERO", FpRound::NearestAway),
    ];
    let mut src = String::from("double d;\n");
    for (i, (m, _)) in modes.iter().enumerate() {
        src.push_str(
            &[
                "double f",
                &i.to_string(),
                "() {\n#pragma STDC FENV_ROUND ",
                m,
                "\n  return d + d;\n}\n",
            ]
            .concat(),
        );
    }
    let tags = tags_of(&src, "c++20", Some(ExprKind::BinaryOperator), true);
    let missing: Vec<_> = modes
        .iter()
        .filter(|(_, r)| !tags.contains(&format!("tag {}", fp_round::row(r).tag)))
        .map(|(m, _)| *m)
        .collect();
    assert!(missing.is_empty(), "not written: {missing:?}; seen {tags:?}");
}
