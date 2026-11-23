//! Back-references (`spec/digest.md` §4.3, WP-049): growth of the shapes
//! whose nodes the walk reaches twice, and the rule's own cases.

use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::stmt::StmtKind;
use slip_kiln::kiln::ast::{ExprId, RawId, StmtId};
use slip_model::Digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};

use crate::stream::Stream;
use crate::tables::walk_row;
use crate::walk::{BACKREFS, Walk};
use crate::{Detail, Input, Options, Output};

/// Digests `src` under `std` on a thread of [`crate::STACK_SIZE`], under
/// `rules`, with the back-references the walk wrote.
fn digest_with(src: &str, std: &str, rules: RuleTable) -> (Output, u64) {
    let src = src.to_owned();
    let std = std.to_owned();
    std::thread::Builder::new()
        .stack_size(crate::STACK_SIZE)
        .spawn(move || {
            let files = [("main.cpp", src.as_str())];
            let snippet = slip_kiln::snippet::Snippet {
                files: &files,
                main: "main.cpp",
                include: &[""],
                std: &std,
            };
            slip_kiln::snippet::parse(&snippet, |p| {
                assert_eq!(p.errors, 0, "{}", p.diagnostics);
                let path = |_| Vec::new();
                let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
                let before = BACKREFS.with(std::cell::Cell::get);
                let o = crate::digest(
                    &input,
                    &Options { rules: &rules, detail: Detail::All, names: true },
                );
                (o, BACKREFS.with(std::cell::Cell::get) - before)
            })
            .expect("parses")
        })
        .expect("a thread")
        .join()
        .expect("no panic")
}

/// `digest.bytes` of `src` (every stream the walk hashed), and the
/// back-references written.
fn bytes(src: &str, std: &str) -> (u64, u64) {
    let (o, refs) = digest_with(src, std, RuleTable::empty());
    (o.counters.iter().find(|c| c.name == "digest.bytes").map_or(0, |c| c.value), refs)
}

/// A designator range over `n` slots whose clause, `n` terms long, needs
/// a conversion in each slot: Kiln gives each slot its own cast around
/// the one clause.
fn range(n: usize) -> String {
    let terms: Vec<String> = (0..n).map(|i| format!("g({i})")).collect();
    format!("int g(int);\nlong v[{n}] = {{[0 ... {}] = {}}};\n", n - 1, terms.join(" + "))
}

/// `n` levels of designated lists in a template (WP-035 "Nesting").
fn nested_lists(n: usize) -> String {
    let mut lines = vec!["struct N0 { int x; };".to_owned()];
    lines.extend((1..=n).map(|i| format!("struct N{i} {{ N{} a; }};", i - 1)));
    let mut e = String::from("{.x = 1}");
    for _ in 1..=n {
        e = format!("{{.a = {e}}}");
    }
    lines.push(format!("template <class T> struct A {{ static inline N{n} w = {e}; }};"));
    lines.join("\n")
}

/// `return s;` of a class type in a constexpr function, `n` times.
fn returns(n: usize) -> String {
    let fs: Vec<String> =
        (0..n).map(|i| format!("constexpr S f{i}() {{ S s; return s; }}\n")).collect();
    let fs = fs.concat();
    format!("struct S {{ constexpr S() {{}} constexpr S(const S &) {{}} }};\n{fs}")
}

/// `return s;` where the move fails and the copy is tried again with `s`
/// as an lvalue (C++20 `kiln_sema` stmt.rs:1917 `return_operand`), each
/// function evaluated in a constant.
fn returns_lvalue(n: usize) -> String {
    let fs: Vec<String> = (0..n)
        .map(|i| {
            format!("constexpr S f{i}() {{ S s; return s; }}\nstatic_assert(f{i}().v == 1);\n")
        })
        .collect();
    let fs = fs.concat();
    format!("struct S {{ constexpr S() {{}} constexpr S(S &) {{}} int v = 1; }};\n{fs}")
}

/// `d` nested expansion statements over `{1}`, in a function.
fn expansions(d: usize) -> String {
    let mut body = String::from(";");
    for i in (0..d).rev() {
        body = format!("template for (int e{i} : {{1}}) {{ {body} }}");
    }
    format!("void f() {{ {body} }}\n")
}

/// `d` nested expansion statements over `{1}`, in a function template.
fn expansions_in_template(d: usize) -> String {
    let mut body = String::from(";");
    for i in (0..d).rev() {
        body = format!("template for (int e{i} : {{1}}) {{ {body} }}");
    }
    format!("template <class T> void f() {{ {body} }}\ntemplate void f<int>();\n")
}

/// `d` nested destructuring expansion statements.
fn destructurings(d: usize) -> String {
    let mut body = String::from(";");
    for i in (0..d).rev() {
        body = format!("template for (auto e{i} : p) {{ {body} }}");
    }
    format!("struct P {{ int a; }};\nvoid f() {{ P p{{1}}; {body} }}\n")
}

/// `d` nested expansion statements over `{1, 2}`: 2^d leaves.
fn expansions_two(d: usize) -> String {
    let mut body = String::from(";");
    for i in (0..d).rev() {
        body = format!("template for (int e{i} : {{1, 2}}) {{ {body} }}");
    }
    format!("void f() {{ {body} }}\n")
}

type Shape = (&'static str, fn(usize) -> String, &'static str, &'static [usize]);

const SHAPES: &[Shape] = &[
    ("range", range, "c++20", &[10, 40, 100, 400]),
    ("nested-lists", nested_lists, "c++20", &[1, 2, 3, 4, 8, 12, 16, 48]),
    ("returns", returns, "c++20", &[1, 4, 16]),
    ("returns-lvalue", returns_lvalue, "c++20", &[1, 4, 16]),
    ("expansions", expansions, "c++26", &[4, 8, 12, 16]),
    ("expansions-in-template", expansions_in_template, "c++26", &[4, 8, 12, 16]),
    ("destructurings", destructurings, "c++26", &[4, 8, 12, 16]),
    ("expansions-two", expansions_two, "c++26", &[4, 8, 12]),
];

/// Prints `digest.bytes` of every shape at each size. A measure:
/// `--ignored --nocapture`.
#[test]
#[ignore = "a measure: --ignored --nocapture"]
fn measure_shapes() {
    for &(name, f, std, sizes) in SHAPES {
        let row: Vec<String> = sizes
            .iter()
            .map(|&n| {
                let (b, refs) = bytes(&f(n), std);
                format!("{n}: {b} ({refs} back-references)")
            })
            .collect();
        println!("{name}: {}", row.join("; "));
    }
}

/// A shape, its language mode, a size `n`, and whether it shares a node.
type Linear = (&'static str, fn(usize) -> String, &'static str, usize, bool);

/// Each shape of WP-049's table, at sizes `n` and `4n`: the bytes grow at
/// most 4.5 times, and the shape writes back-references (`return s;`
/// shares no node in this Kiln: linear without one). The nested
/// designated lists stay quadratic: `expr_init`'s
/// `nested_designated_lists`.
#[test]
fn the_shapes_are_linear() {
    let shapes: [Linear; 6] = [
        ("range", range, "c++20", 10, true),
        ("expansions", expansions, "c++26", 4, true),
        ("expansions-in-template", expansions_in_template, "c++26", 4, true),
        ("destructurings", destructurings, "c++26", 4, true),
        ("returns", returns, "c++20", 4, false),
        ("range-large", range, "c++20", 100, true),
    ];
    for (name, f, std, n, shared) in shapes {
        let (small, refs) = bytes(&f(n), std);
        let (big, _) = bytes(&f(4 * n), std);
        eprintln!("{name}: {n}: {small} bytes; {}: {big} bytes", 4 * n);
        assert_eq!(refs > 0, shared, "{name}: {refs} back-references");
        assert!(2 * big <= 9 * small, "{name}: {n}: {small} bytes; {}: {big} bytes", 4 * n);
    }
}

/// Parses `src` and runs `f` on a fresh walk over it.
fn with_walk<T>(src: &str, std: &str, f: impl FnOnce(&mut Walk<'_>) -> T) -> T {
    let files = [("main.cpp", src)];
    let snippet =
        slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let path = |_| Vec::new();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let rules = RuleTable::empty();
        let mut w = Walk::new(&input, &rules);
        f(&mut w)
    })
    .expect("parses")
}

/// The last expression of kind `k`.
fn last_expr(w: &Walk<'_>, k: ExprKind) -> ExprId {
    (1..=w.ast.exprs.len())
        .rev()
        .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
        .find(|&e| w.ast.exprs.get(e).kind() == k)
        .unwrap_or_else(|| panic!("no {k:?}"))
}

/// The first statement of kind `k`.
fn first_stmt(w: &Walk<'_>, k: StmtKind) -> StmtId {
    (1..=w.ast.stmts.len())
        .filter_map(|i| StmtId::try_new(u32::try_from(i).ok()?))
        .find(|&st| w.ast.stmts.get(st).kind() == k)
        .unwrap_or_else(|| panic!("no {k:?}"))
}

/// The bytes `child` of `e` appends to `s`.
fn appended(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) -> Vec<u8> {
    let at = s.as_bytes().len();
    crate::expr::child(w, s, e);
    s.as_bytes()[at..].to_vec()
}

/// `tag BACKREF`, `uv n`.
fn backref(n: u64) -> Vec<u8> {
    let mut s = Stream::new();
    s.uv(u64::from(walk_row::BACKREF)).uv(n);
    s.as_bytes().to_vec()
}

/// §4.3: the first writing of a node is what it was without the rule, a
/// second writing in the same stream and state is `tag BACKREF`, `uv`
/// its count, and a stream counts from 0 at its head.
#[test]
fn a_node_written_again_is_a_back_reference() {
    with_walk("int a;\nint v = a + 1;\n", "c++20", |w| {
        let e = last_expr(w, ExprKind::BinaryOperator);
        let mut s = Stream::new();
        let first = appended(w, &mut s, e);
        assert_eq!(appended(w, &mut s, e), backref(0));
        // The operand `a` (under its conversion) was counted after `e`.
        let lhs = ExprId::try_new(w.ast.exprs.get(e).a()).expect("lhs");
        assert_eq!(appended(w, &mut s, lhs), backref(1));
        // A stream of its own counts from 0 and writes the node in full.
        let mut t = Stream::new();
        assert_eq!(appended(w, &mut t, e), first);
        assert_eq!(appended(w, &mut t, e), backref(0));
        // The first stream's memo is kept while the other is written.
        assert_eq!(appended(w, &mut s, e), backref(0));
    });
}

/// §4.3: statements and expressions are counted in one sequence
/// (mutation: a count per kind).
#[test]
fn statements_and_expressions_are_counted_in_one_sequence() {
    with_walk("int a;\nvoid f() { a = 1; }\n", "c++20", |w| {
        let body = first_stmt(w, StmtKind::CompoundStmt);
        let stmt = first_stmt(w, StmtKind::ExprStmt);
        let e = ExprId::try_new(w.ast.stmts.get(stmt).a()).expect("the expression");
        let mut s = Stream::new();
        crate::stmt::child(w, &mut s, body);
        // The block is 0, the expression statement 1, its expression 2.
        assert_eq!(appended(w, &mut s, e), backref(2));
        let at = s.as_bytes().len();
        crate::stmt::child(w, &mut s, stmt);
        assert_eq!(s.as_bytes()[at..].to_vec(), backref(1));
    });
}

/// §4.3: a statement and an expression whose ids are the same number are
/// two nodes (mutation: the memo's key without the statement bit). The
/// statement is a null statement, which holds no expression.
#[test]
fn a_statement_and_an_expression_of_one_number_are_two_nodes() {
    with_walk("int a;\nvoid f() { ; ; ; ; ; ; ; ; }\nint v = a + 1;\n", "c++20", |w| {
        let st = (1..=w.ast.stmts.len())
            .filter_map(|i| StmtId::try_new(u32::try_from(i).ok()?))
            .find(|&st| w.ast.stmts.get(st).kind() == StmtKind::NullStmt)
            .expect("a null statement");
        let e = ExprId::try_new(st.raw()).expect("an expression of the same number");
        let mut fresh = Stream::new();
        crate::stmt::child(w, &mut fresh, st);
        let mut s = Stream::new();
        crate::expr::child(w, &mut s, e);
        let at = s.as_bytes().len();
        crate::stmt::child(w, &mut s, st);
        assert_eq!(s.as_bytes()[at..].to_vec(), fresh.as_bytes().to_vec());
    });
}

/// §4.3 "the state": a node reached again in another state is written in
/// full and counted again; reached again in a state it was written in, it
/// is that writing's back-reference (mutation: the state left out of the
/// key). Profile mode counts nothing.
#[test]
fn a_node_in_another_state_is_written_again() {
    let src = "struct S { int m; int f() { return m; } };\n";
    with_walk(src, "c++20", |w| {
        let this = last_expr(w, ExprKind::CXXThisExpr);
        let alone = |w: &mut Walk<'_>, dependent: u32, in_key: u32| {
            let saved = w.take_expr_state();
            w.unit.expr.dependent = dependent;
            w.in_key = in_key;
            let mut t = Stream::new();
            let b = appended(w, &mut t, this);
            w.in_key = 0;
            w.unit.expr = saved;
            b
        };
        let (plain, dependent, keyed) = (alone(w, 0, 0), alone(w, 1, 0), alone(w, 0, 1));
        assert_ne!(plain, dependent, "aux-implicit is written in a dependent expression");
        let mut s = Stream::new();
        assert_eq!(appended(w, &mut s, this), plain);
        w.unit.expr.dependent = 1;
        assert_eq!(appended(w, &mut s, this), dependent, "another state: in full");
        assert_eq!(appended(w, &mut s, this), backref(1));
        w.unit.expr.dependent = 0;
        assert_eq!(appended(w, &mut s, this), backref(0));
        w.in_key = 1;
        assert_eq!(appended(w, &mut s, this), keyed, "in a key: in full");
        assert_eq!(appended(w, &mut s, this), backref(2));
        w.in_key = 0;
        w.unit.expr.profile = Some(crate::expr_dep::Frame::default());
        let profiled = appended(w, &mut s, this);
        let alone = {
            let mut t = Stream::new();
            appended(w, &mut t, this)
        };
        assert_eq!(profiled, alone, "profile mode: a node written before is written in full");
        assert_eq!(appended(w, &mut s, this), profiled, "profile mode: neither counted nor named");
        w.unit.expr.profile = None;
        assert_eq!(appended(w, &mut s, this), backref(0));
    });
}

/// The pair WP-049 asks for, as a unit test: an implicit `this` against
/// an explicit one, each written once outside a dependent expression
/// (where `aux-implicit` is not written) and once inside one: the streams
/// differ (mutation: the state left out of the key makes them equal).
/// Sources do reach one node in both states
/// (`a_node_shared_by_a_dependent_pattern_and_its_expansions`), but there
/// the dependent reach comes first, so a pair of sources cannot be made
/// equal by the mutation: this order is reached only here.
#[test]
fn shared_node_in_two_states() {
    let stream = |src: &str| {
        with_walk(src, "c++20", |w| {
            let this = last_expr(w, ExprKind::CXXThisExpr);
            let mut s = Stream::new();
            crate::expr::child(w, &mut s, this);
            w.unit.expr.dependent = 1;
            crate::expr::child(w, &mut s, this);
            w.unit.expr.dependent = 0;
            let first = {
                let mut t = Stream::new();
                crate::expr::child(w, &mut t, this);
                t.as_bytes().to_vec()
            };
            (s.as_bytes().to_vec(), first)
        })
    };
    let (implicit, a) = stream("struct S { int m; int f() { return m; } };\n");
    let (explicit, b) = stream("struct S { int m; int f() { return this->m; } };\n");
    assert_eq!(a, b, "outside a dependent expression the two are written alike");
    assert_ne!(implicit, explicit, "the second state's row is written");
}

/// §4.3 "opaque values come first": a node numbered as an opaque value's
/// source is written as today, not as a back-reference (mutation: a
/// back-reference for a source), and the opaque values of `a ?: b`,
/// reached twice, write their number (mutation: an opaque value in the
/// memo).
#[test]
fn opaque_values_come_first() {
    with_walk("int a;\nint v = a + 1;\n", "c++20", |w| {
        let e = last_expr(w, ExprKind::BinaryOperator);
        let mut s = Stream::new();
        let first = appended(w, &mut s, e);
        let unit = &mut w.unit.expr;
        unit.opaques.insert(e, unit.next_opaque);
        unit.next_opaque += 1;
        let again = appended(w, &mut s, e);
        // Its own rows again (its operands, written before, are named).
        assert_ne!(again, backref(0), "a numbered source is not a back-reference");
        assert_eq!(again[..2], first[..2], "a numbered source is written as a node");
    });
    with_walk("int a;\nint v = a ?: 1;\n", "c++20", |w| {
        let e = last_expr(w, ExprKind::BinaryConditionalOperator);
        let before = BACKREFS.with(std::cell::Cell::get);
        let mut s = Stream::new();
        crate::expr::child(w, &mut s, e);
        assert_eq!(BACKREFS.with(std::cell::Cell::get), before, "no back-reference");
    });
}

/// §4.4: a stream written inside another (a type digest) neither sees the
/// memo of the stream around it nor loses it (mutations: the memo not put
/// back after an inner stream; the stream's identity left out of the
/// key), and a stream written alone under the same state does not name a
/// node of another.
#[test]
fn inner_streams_neither_see_nor_lose_the_memo() {
    with_walk("int a;\nlong v = a + 1;\n", "c++20", |w| {
        let e = last_expr(w, ExprKind::BinaryOperator);
        let cast = last_expr(w, ExprKind::ImplicitCastExpr);
        let long = w.ast.exprs.get(cast).ty();
        let mut s = Stream::new();
        let first = appended(w, &mut s, e);
        let t = w.type_digest(long, false);
        assert_eq!(t.cause, None);
        assert_eq!(appended(w, &mut s, e), backref(0), "the memo is back after a type digest");
        // A canonical operand written into the same stream (profile mode,
        // with the expression state put aside): the memo is back after it.
        crate::expr_dep::profile_expr(w, &mut s, e);
        assert_eq!(appended(w, &mut s, e), backref(0), "the memo is back after a profile");
        let mut alone = Stream::new();
        assert_eq!(appended(w, &mut alone, e), first, "another stream writes the node in full");
    });
}

/// §4.4: a unit whose stream is written inside another's, with a shared
/// node, has the digest it has when written first: the memo starts empty
/// at the head of the inner unit and the outer one's is back after it.
/// The inner unit is a `static_assert`, keyed by its content digest
/// (`Walk::content_hash`), determinate, written while another stream is
/// open; the first digest is from a walk that writes nothing before it.
#[test]
fn a_unit_written_inside_another_is_the_same() {
    let src = "struct A { long v[3]; };\n\
               static_assert(A{{[0 ... 2] = 1 + 2}}.v[2] == 3);\n\
               long v[3] = {[0 ... 2] = 1 + 2};\n";
    let lone = with_walk(src, "c++20", |w| {
        let sa = static_assert(w);
        let refs = BACKREFS.with(std::cell::Cell::get);
        let h = w.content_hash(sa);
        assert!(BACKREFS.with(std::cell::Cell::get) > refs, "the unit has a shared node");
        h
    });
    assert_eq!(lone.cause, None, "the unit is determinate");
    with_walk(src, "c++20", |w| {
        let sa = static_assert(w);
        let list = last_expr(w, ExprKind::InitListExpr);
        let mut outer = Stream::new();
        let first = appended(w, &mut outer, list);
        let refs = BACKREFS.with(std::cell::Cell::get);
        let inner = w.content_hash(sa);
        assert!(BACKREFS.with(std::cell::Cell::get) > refs, "the inner unit has a shared node");
        assert_eq!(
            (inner.digest, inner.cause),
            (lone.digest, lone.cause),
            "written inside another stream, the same digest"
        );
        assert_eq!(appended(w, &mut outer, list), backref(0), "the outer memo is back");
        assert!(first.len() > backref(0).len());
    });
}

/// The first `static_assert`.
fn static_assert(w: &Walk<'_>) -> slip_kiln::kiln::ast::DeclId {
    let ast = w.ast;
    (1..=ast.decls.len())
        .filter_map(|i| slip_kiln::kiln::ast::DeclId::try_new(u32::try_from(i).ok()?))
        .find(|&d| ast.decls.get(d).kind() == slip_kiln::kiln::ast::decl::DeclKind::StaticAssert)
        .expect("the static_assert")
}

/// The source of `equal/_/backref-content-unit`, side A and side B (the
/// shared clause in redundant parentheses): a `static_assert` in a class,
/// content-keyed, whose digest is computed inside the class's stream.
const CONTENT_UNIT: [&str; 2] = [
    "struct A { long v[3]; };\nstruct S { static_assert(A{{[0 ... 2] = 1 + 2}}.v[2] == 3); };\n",
    "struct A { long v[3]; };\nstruct S { static_assert(A{{[0 ... 2] = (1 + 2)}}.v[2] == 3); };\n",
];

/// `equal/_/backref-content-unit` shows what it says: on each side every
/// unit is determinate and the walk writes back-references (the
/// conversions of the three slots share the clause), and the two sides'
/// units are equal.
#[test]
fn the_content_unit_pair_shares_nodes() {
    let units = |src: &str| {
        let (o, refs) = digest_with(src, "c++20", RuleTable::empty());
        let units: Vec<slip_model::digests::Unit> =
            o.digests.groups.iter().flat_map(|g| g.units.iter().flatten().copied()).collect();
        assert!(refs > 0, "{refs} back-references: {src}");
        assert!(units.iter().all(|u| !u.is_indeterminate()), "{src}");
        units.iter().map(|u| (u.key, u.digest)).collect::<Vec<_>>()
    };
    assert_eq!(units(CONTENT_UNIT[0]), units(CONTENT_UNIT[1]));
}

/// §4.3: a back-reference's count is the node's number in its stream,
/// counted from 0 at the first node, whatever bytes come before it
/// (mutation: the count started at the stream's length). The unit stream
/// of a `static_assert` writes its rows and then the condition, which
/// holds a shared node: the condition written alone, from a stream's
/// head, is the same bytes, back-reference and all.
#[test]
fn a_back_reference_in_a_unit_stream_counts_from_its_first_node() {
    let src = "struct A { long v[3]; };\nstatic_assert(A{{[0 ... 2] = 1 + 2}}.v[2] == 3);\n";
    with_walk(src, "c++20", |w| {
        let sa = static_assert(w);
        let Some(&slip_kiln::kiln::ast::decl::StaticAssertData { cond, .. }) =
            w.ast.decls.static_assert(sa)
        else {
            panic!("no condition")
        };
        let lone = {
            let saved = w.take_expr_state();
            let mut t = Stream::new();
            let b = appended(w, &mut t, cond);
            w.unit.expr = saved;
            b
        };
        let saved = w.take_expr_state();
        let mut u = Stream::new();
        let refs = BACKREFS.with(std::cell::Cell::get);
        w.decl_stream(&mut u, sa, sa);
        w.unit.expr = saved;
        assert!(BACKREFS.with(std::cell::Cell::get) > refs, "the unit has a shared node");
        let unit = u.as_bytes();
        let at = unit.windows(lone.len()).position(|x| x == lone.as_slice());
        assert!(matches!(at, Some(at) if at > 0), "the condition, after the unit's rows: {at:?}");
        // The clause `1 + 2` is the shared node: written with the first
        // slot, then named by the other two.
        let clause = (1..=w.ast.exprs.len())
            .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
            .find(|&e| w.ast.exprs.get(e).kind() == ExprKind::BinaryOperator)
            .expect("the clause");
        let named = {
            let saved = w.take_expr_state();
            let mut t = Stream::new();
            crate::expr::child(w, &mut t, cond);
            let b = appended(w, &mut t, clause);
            w.unit.expr = saved;
            b
        };
        assert_eq!(named[..2], backref(0)[..2], "the clause is a back-reference");
        let hits = unit.windows(named.len()).filter(|x| *x == named.as_slice()).count();
        assert_eq!(hits, 2, "the other two slots name the clause by its count");
    });
}

/// §4.3, §7 "Parentheses": the memo's key is the node after its redundant
/// parentheses are skipped (mutation: the `ParenExpr` keyed as itself).
/// One `ParenExpr` reached kept (a root that reads it) and then skipped:
/// the second reach names its operand, written inside the first.
#[test]
fn a_skipped_paren_is_keyed_as_its_operand() {
    with_walk("int a;\nint v = (a + 1) * 2;\n", "c++20", |w| {
        let paren = last_expr(w, ExprKind::ParenExpr);
        let kept = {
            let mut t = Stream::new();
            crate::expr::child_keep_paren(w, &mut t, paren);
            t.as_bytes().to_vec()
        };
        let skipped = {
            let mut t = Stream::new();
            appended(w, &mut t, paren)
        };
        assert_ne!(kept, skipped, "the root keeps its parentheses");
        let mut s = Stream::new();
        crate::expr::child_keep_paren(w, &mut s, paren);
        assert_eq!(s.as_bytes(), kept.as_slice());
        // The `ParenExpr` is 0, its operand `a + 1` is 1.
        assert_eq!(appended(w, &mut s, paren), backref(1), "skipped: its operand's count");
        let mut s = Stream::new();
        assert_eq!(appended(w, &mut s, paren), skipped);
        let at = s.as_bytes().len();
        crate::expr::child_keep_paren(w, &mut s, paren);
        let tail = &s.as_bytes()[at..];
        assert!(tail.windows(3).any(|x| x == backref(0).as_slice()), "kept: its operand named");
        assert_ne!(s.as_bytes()[at..].to_vec(), backref(0));
    });
}

/// §4.3 "the state": one node reached both inside and outside an
/// instantiation-dependent expression, in a real source. An expansion
/// statement writes its dependent pattern and then its expansions, which
/// share nodes with it: the implicit `this` and its member access of
/// `g(m + e)`, and the call `g(1)` of `g(g(1) + e)`, are first written
/// inside the dependent pattern and reached again outside it, where they
/// are written in full and counted again (mutation: the dependent bit
/// left out of the key makes the second reach a back-reference to the
/// first).
#[test]
fn a_node_shared_by_a_dependent_pattern_and_its_expansions() {
    let cases = [
        (
            "int g(int);\nstruct S { int m; void k() { template for (auto e : {1, 2}) { g(m + e); } } };\n",
            ExprKind::MemberExpr,
        ),
        (
            "int g(int);\nvoid k() { template for (auto e : {1, 2}) { g(g(1) + e); } }\n",
            ExprKind::CallExpr,
        ),
    ];
    for (src, kind) in cases {
        with_walk(src, "c++26", |w| {
            let expansion = first_stmt(w, StmtKind::CXXExpansionStmt);
            let mut s = Stream::new();
            crate::stmt::child(w, &mut s, expansion);
            let both = (1..=w.ast.exprs.len())
                .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
                .filter(|&e| w.ast.exprs.get(e).kind() == kind)
                .filter(|&e| {
                    w.unit.expr.dependent = 0;
                    let outside = appended(w, &mut s, e);
                    w.unit.expr.dependent = 1;
                    let inside = appended(w, &mut s, e);
                    w.unit.expr.dependent = 0;
                    let named = |b: &[u8]| b[..2] == backref(0)[..2];
                    named(&outside) && named(&inside) && outside != inside
                })
                .count();
            assert!(both > 0, "a {kind:?} written in both states: {src}");
        });
    }
}

fn rename_rule(from: &str, to: &str) -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: Name::Identifier(from.as_bytes().to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    }
}

/// §4.3 "why it cannot hide a difference": a namespace relocated by a
/// rule, whose units hold shared nodes naming its entities, gives the
/// digests of the same source written under the new name (the sharing
/// does not move with a name).
#[test]
fn shared_nodes_under_a_relocation_rule() {
    let src = |ns: &str| {
        format!(
            "namespace {ns} {{ int g(int); struct S {{ int x; }};\n\
             long v[8] = {{[0 ... 7] = g(1) + g(2)}};\n\
             void f() {{ template for (S e : {{S{{1}}, S{{2}}}}) {{ g(e.x); }} }} }}\n"
        )
    };
    let keys = |o: &Output| -> Vec<(Digest, Digest)> {
        let mut v: Vec<_> = o
            .digests
            .groups
            .iter()
            .flat_map(|g| g.units.iter().flatten().map(|u| (u.key, u.digest)))
            .collect();
        v.sort();
        v
    };
    let rules = RuleTable::new(vec![rename_rule("old", "neu")]).expect("a valid rule");
    let (mapped, refs) = digest_with(&src("old"), "c++26", rules);
    let (plain, plain_refs) = digest_with(&src("neu"), "c++26", RuleTable::empty());
    assert!(refs > 0 && refs == plain_refs, "{refs} and {plain_refs} back-references");
    assert_eq!(keys(&mapped), keys(&plain));
    let (unmapped, _) = digest_with(&src("old"), "c++26", RuleTable::empty());
    assert_ne!(keys(&unmapped), keys(&plain));
}
