//! `spec/api.md` §3 (the operation table, the result word, handles, kind
//! bits, statuses), §4 (the status values), §5.2 (forged values), the query
//! half of §3.2, and §7.6 (the canonical form of the run output), against
//! the fake subject. Each test names its section.

#![allow(
    clippy::many_single_char_names,
    clippy::disallowed_names,
    reason = "the subject is the specification's example: lib::foo(a, b)"
)]

use std::path::Path;

use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::{Counter, Digest, FileRef};
use slip_script::fake::{
    FakeCall, FakeDeclaration, FakeEntity, FakeExpression, FakeOut, FakeReference, FakeSubject,
};
use slip_script::host::{Host, KEYWORDS, Limits, check_identifier, parse_qualified_name};
use slip_script::output::{Changed, Edit, Piece, Place, Report, RunOutput, Severity};
use slip_script::subject::{
    Declaration, Entity, Expression, File, Range, Reference, Subject, Value,
};
use slip_script::wire::{self, Arity, Convention, Errc};

// ---------------------------------------------------------------------------
// The subject of most tests.

const T_CPP: &str = "namespace lib { inline namespace inl { int foo(int a, int b); } }\n\
namespace { int hidden; }\n\
int main() { return lib::foo(1, 2) + lib::foo(3, 4); }\n";
const H_H: &str = "struct S;\n";

#[allow(dead_code, reason = "a description; not every test uses every name")]
struct World {
    fake: FakeSubject,
    t: File,
    h: File,
    lib: Entity,
    inl: Entity,
    foo: Entity,
    a: Entity,
    b: Entity,
    anon: Entity,
    hidden: Entity,
    main: Entity,
    foo_spec: Entity,
    s: Entity,
}

fn world() -> World {
    let mut f = FakeSubject::new();
    let g = FakeSubject::GLOBAL;
    let t = f.file("/src/t.cpp", T_CPP, true);
    let h = f.file("/src/h.h", H_H, false);
    let s = f.entity(FakeEntity::class("S", g));
    let lib = f.entity(FakeEntity::namespace("lib", g));
    let inl = f.entity(FakeEntity { transparent: true, ..FakeEntity::namespace("inl", lib) });
    let foo = f.entity(FakeEntity {
        key: Some(Digest([7; 32])),
        ..FakeEntity::function("foo", inl, Vec::new())
    });
    let a = f.entity(FakeEntity::parameter("a", foo));
    let b = f.entity(FakeEntity::parameter("b", foo));
    f.entities[foo.0 as usize].parameters = Some(vec![a, b]);
    let anon = f.entity(FakeEntity { transparent: true, ..FakeEntity::namespace("", g) });
    let hidden = f.entity(FakeEntity {
        identifier: b"hidden".to_vec(),
        kinds: wire::KIND_VARIABLE,
        parent: Some(anon),
        ..FakeEntity::default()
    });
    let main = f.entity(FakeEntity::function("main", g, Vec::new()));
    let foo_spec =
        f.entity(FakeEntity { origin: Some(foo), ..FakeEntity::function("foo", inl, vec![a, b]) });

    let decl = |f: &mut FakeSubject, entity, name: Option<Range>, definition| {
        f.declaration(FakeDeclaration { entity, name, definition });
    };
    let n = |f: &FakeSubject, file, needle, nth| Some(f.range_nth(file, needle, nth));
    let r = n(&f, h, "S", 0);
    decl(&mut f, s, r, false);
    let r = n(&f, t, "lib", 0);
    decl(&mut f, lib, r, true);
    let r = n(&f, t, "inl", 0);
    decl(&mut f, inl, r, true);
    let r = n(&f, t, "foo", 0);
    decl(&mut f, foo, r, false);
    let r = n(&f, t, "int a", 0).map(|r| Range { lo: r.lo + 4, ..r });
    decl(&mut f, a, r, true);
    let r = n(&f, t, "int b", 0).map(|r| Range { lo: r.lo + 4, ..r });
    decl(&mut f, b, r, true);
    decl(&mut f, anon, None, true);
    let r = n(&f, t, "hidden", 0);
    decl(&mut f, hidden, r, true);
    let r = n(&f, t, "main", 0);
    decl(&mut f, main, r, true);

    let r_lib1 = f.reference(FakeReference {
        name: n(&f, t, "lib", 1),
        extent: n(&f, t, "lib", 1),
        ..FakeReference::to(lib, "lib")
    });
    let _ = r_lib1;
    let r_foo1 = f.reference(FakeReference {
        name: n(&f, t, "foo", 1),
        extent: n(&f, t, "lib::foo", 0),
        ..FakeReference::to(foo, "foo")
    });
    let r_foo2 = f.reference(FakeReference {
        name: n(&f, t, "foo", 2),
        extent: n(&f, t, "lib::foo", 1),
        ..FakeReference::to(foo_spec, "foo")
    });
    let c1 = f.expressions.len() as u32;
    let call = |f: &mut FakeSubject, text: &str, callee, reference, args: [&str; 2], first: u32| {
        let extent = Some(f.range(t, text));
        let call = FakeCall {
            callee,
            reference,
            arguments: vec![Expression(first + 1), Expression(first + 2)],
        };
        f.expression(FakeExpression { call: Some(call), ..FakeExpression::at(extent) });
        for a in args {
            let r = Some(f.range(t, a));
            f.expression(FakeExpression::at(r));
        }
    };
    call(&mut f, "lib::foo(1, 2)", foo, r_foo1, ["1", "2"], c1);
    call(&mut f, "lib::foo(3, 4)", foo_spec, r_foo2, ["3", "4"], c1 + 3);
    World { fake: f, t, h, lib, inl, foo, a, b, anon, hidden, main, foo_spec, s }
}

fn host(f: &FakeSubject) -> Host<'_> {
    Host::new(f, Digest([1; 32]), Limits::default())
}

const ROOM: usize = 1 << 12;

/// A call with room for any answer of these tests.
fn call(h: &mut Host<'_>, op: u32, ints: &[u64], bytes: &[u8]) -> (u64, FakeOut) {
    let mut out = FakeOut::with_capacity(ROOM, ROOM);
    let word = h.call(op, ints, bytes, &mut out).expect("no host failure");
    (word, out)
}

fn status(h: &mut Host<'_>, op: u32, ints: &[u64], bytes: &[u8]) -> u8 {
    wire::status(call(h, op, ints, bytes).0)
}

/// A successful call of convention V: its value.
fn v(h: &mut Host<'_>, op: u32, ints: &[u64]) -> u64 {
    let (word, out) = call(h, op, ints, &[]);
    assert_eq!(wire::status(word), 0, "op {op} failed: {:?}", h.failures().last());
    assert_eq!(out.writes, 0, "convention V writes nothing");
    wire::payload(word)
}

/// A successful call of convention B: the integers.
fn ints(h: &mut Host<'_>, op: u32, args: &[u64], bytes: &[u8]) -> Vec<u64> {
    let (word, out) = call(h, op, args, bytes);
    assert_eq!(wire::status(word), 0, "op {op} failed: {:?}", h.failures().last());
    assert_eq!(wire::n_ints(word) as usize, out.ints.len());
    assert_eq!(wire::n_bytes(word), 0);
    out.ints
}

/// A successful call of convention B: the bytes.
fn bytes(h: &mut Host<'_>, op: u32, args: &[u64]) -> Vec<u8> {
    let (word, out) = call(h, op, args, &[]);
    assert_eq!(wire::status(word), 0, "op {op} failed: {:?}", h.failures().last());
    assert_eq!(wire::n_bytes(word) as usize, out.bytes.len());
    assert_eq!(wire::n_ints(word), 0);
    out.bytes
}

fn lookup(h: &mut Host<'_>, name: &str) -> Vec<u64> {
    ints(h, wire::OP_LOOKUP, &[], name.as_bytes())
}

fn one(h: &mut Host<'_>, name: &str) -> u64 {
    let r = lookup(h, name);
    assert_eq!(r.len(), 1, "{name}");
    r[0]
}

fn errc(e: Errc) -> u8 {
    e.status()
}

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

// ---------------------------------------------------------------------------
// §3.2, §3.4: the table against the header and the specification.

/// §3.2, §3.4: every operation number and kind bit of `__intrinsics.h`
/// equals the constant here, and the other way round.
#[test]
fn s3_2_s3_4_header_constants_equal_the_table() {
    let path = repo().join("resource/include/slip/__intrinsics.h");
    let text = std::fs::read_to_string(&path).expect("read __intrinsics.h");
    let mut ops = Vec::new();
    let mut kinds = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("__op_") {
            let (name, value) = rest.split_once('=').expect("__op_name = N,");
            let value = value.trim().trim_end_matches(',').parse::<u32>().expect("a number");
            ops.push((name.trim().to_owned(), value));
        } else if let Some(rest) = line.strip_prefix("__kind_") {
            let (name, value) = rest.split_once('=').expect("__kind_name = 1ull << N,");
            let shift = value
                .trim()
                .trim_end_matches(',')
                .strip_prefix("1ull <<")
                .expect("1ull << N")
                .trim()
                .parse::<u32>()
                .expect("a shift");
            kinds.push((name.trim().to_owned(), 1u64 << shift));
        }
    }
    let table: Vec<(String, u32)> =
        wire::OPERATIONS.iter().map(|o| (o.name.to_owned(), o.op)).collect();
    assert_eq!(ops, table, "operations of the header and of wire::OPERATIONS");
    let ours: Vec<(String, u64)> = wire::KINDS.iter().map(|&(n, b)| (n.to_owned(), b)).collect();
    assert_eq!(kinds, ours, "kind bits of the header and of wire::KINDS");
    // The named constants are the ones in the tables.
    for (op, name) in [
        (wire::OP_ERROR_MESSAGE, "error_message"),
        (wire::OP_LOOKUP, "lookup"),
        (wire::OP_ARGUMENTS_AS_WRITTEN, "arguments_as_written"),
        (wire::OP_EXPECT_RENAMED, "expect_renamed"),
    ] {
        assert_eq!(wire::operation(op).map(|o| o.name), Some(name));
    }
}

/// §3.2: each row of the specification's table has the number, name,
/// integer count, text and convention of the table here.
#[test]
fn s3_2_table_equals_the_specification() {
    let text = std::fs::read_to_string(repo().join("spec/api.md")).expect("read spec/api.md");
    let start = text.find("### 3.2 Operations").expect("§3.2");
    let end = start + text[start..].find("### 3.3").expect("§3.3");
    let mut rows = Vec::new();
    for line in text[start..end].lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() != 7 {
            continue;
        }
        let Ok(op) = cells[1].parse::<u32>() else { continue };
        let name = cells[2].trim_matches('`');
        let n_in = cells[3];
        let ints = if n_in.is_empty() {
            Arity::Exactly(0)
        } else if n_in.contains("any number") {
            Arity::Any
        } else {
            Arity::Exactly(n_in.split(',').map(|a| if a.trim() == "`r`" { 2 } else { 1 }).sum())
        };
        let text = !cells[4].is_empty();
        let convention = if cells[5].starts_with("V:") { Convention::V } else { Convention::B };
        rows.push((op, name.to_owned(), ints, text, convention));
    }
    let ours: Vec<_> = wire::OPERATIONS
        .iter()
        .map(|o| (o.op, o.name.to_owned(), o.ints, o.text, o.convention))
        .collect();
    assert_eq!(rows, ours);
    for o in wire::OPERATIONS {
        assert_eq!(o.effect, o.op >= 64, "{}", o.name);
    }
}

/// §4: the status values of `errc`; §3.5 rule 1: 254 and 255.
#[test]
fn s4_errc_status_values() {
    let all = [
        (Errc::InvalidHandle, 1, "invalid_handle"),
        (Errc::WrongKind, 2, "wrong_kind"),
        (Errc::InvalidArgument, 3, "invalid_argument"),
        (Errc::NotEditable, 4, "not_editable"),
        (Errc::Unsupported, 5, "unsupported"),
        (Errc::LimitExceeded, 6, "limit_exceeded"),
    ];
    for (e, n, name) in all {
        assert_eq!(e.status(), n);
        assert_eq!(e as u8, n);
        assert_eq!(e.name(), name);
    }
    assert_eq!(wire::STATUS_UNKNOWN_OP, 254);
    assert_eq!(wire::STATUS_MALFORMED, 255);
}

/// §3.3: the result word's fields.
#[test]
fn s3_3_result_word() {
    let w = wire::failure_word(3, 17);
    assert_eq!((wire::status(w), wire::payload(w)), (3, 17));
    let w = wire::counts_word(5, 9);
    assert_eq!((wire::status(w), wire::n_bytes(w), wire::n_ints(w)), (0, 5, 9));
    let w = wire::counts_word((1 << 28) - 1, (1 << 28) - 1);
    assert_eq!(wire::status(w), 0);
    assert_eq!((wire::n_bytes(w), wire::n_ints(w)), ((1 << 28) - 1, (1 << 28) - 1));
    assert_eq!(wire::value_word(wire::PAYLOAD_MASK), wire::PAYLOAD_MASK);
}

// ---------------------------------------------------------------------------
// Rule 1: handles (§3.4, §5.2).

/// §3.4: a handle is `tag << 48 | index`, its tag the value's sort.
#[test]
fn s3_4_handles_are_tag_and_index() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    assert_eq!(foo >> 48, u64::from(wire::TAG_ENTITY));
    let decls = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[]);
    assert_eq!(decls[0] >> 48, u64::from(wire::TAG_DECLARATION));
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    assert!(refs.iter().all(|r| r >> 48 == u64::from(wire::TAG_REFERENCE)));
    let calls = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    assert!(calls.iter().all(|c| c >> 48 == u64::from(wire::TAG_EXPRESSION)));
    assert!([foo, decls[0], refs[0], calls[0]].iter().all(|&x| x != 0 && x <= wire::PAYLOAD_MASK));
}

/// §5.1 rule 2: one value has one handle, however it was reached; an
/// entity is not equal to its declaration.
#[test]
fn s5_1_one_value_one_handle() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    assert_eq!(one(&mut h, "::lib::inl::foo"), foo);
    let decl = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_ne!(decl, foo);
    assert_eq!(v(&mut h, wire::OP_ENTITY_OF, &[decl]), foo);
    let calls = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_CALLEE_OF, &[calls[0]]), foo);
}

/// §3.4, §5.2 rule 2: every number the host did not issue in this run is
/// `invalid_handle`, wherever an info is required.
#[test]
fn s5_2_forged_handles_are_invalid_handle() {
    let w = world();
    let mut other = host(&w.fake);
    // Issue many entities in another host, so their indices exist there.
    for name in ["S", "lib", "lib::inl", "lib::foo", "main", "hidden"] {
        one(&mut other, name);
    }
    let foreign = one(&mut other, "main");
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo"); // index 0 here
    let forged = [
        0,
        wire::handle(0, 0),
        wire::handle(wire::TAG_ENTITY, 1), // not issued yet
        foreign,                           // issued by another host only
        wire::handle(wire::TAG_DECLARATION, 0),
        wire::handle(wire::TAG_CODE, 0),
        wire::handle(wire::TAG_GROUP, 0),
        wire::handle(7, 0),
        wire::handle(255, 0),
        foo | 1 << 56,
        u64::MAX,
        wire::handle(wire::TAG_ENTITY, wire::INDEX_MASK),
    ];
    let info_ops = wire::OPERATIONS.iter().filter(|o| !o.effect && o.ints == Arity::Exactly(1));
    for o in info_ops.filter(|o| o.op != wire::OP_ERROR_MESSAGE) {
        for &x in &forged {
            assert_eq!(
                status(&mut h, o.op, &[x], &[]),
                errc(Errc::InvalidHandle),
                "{} of {x:#x}",
                o.name
            );
        }
    }
    // The live value still works.
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[foo]), one(&mut h, "lib::inl"));
}

/// §5.2 rule 2: forged bits equal to a live value's are that value.
#[test]
fn s5_2_forged_bits_equal_to_a_live_value_are_that_value() {
    let w = world();
    let mut h = host(&w.fake);
    let lib = one(&mut h, "lib");
    let forged = wire::handle(wire::TAG_ENTITY, wire::handle_index(lib));
    assert_eq!(forged, lib);
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[forged]), b"lib");
}

/// §3.4: a range is validated: the file handle issued in this run, and
/// `lo <= hi <=` the file's length; the null range is refused.
#[test]
fn s3_4_ranges_are_validated() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let decl = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    let r = ints(&mut h, wire::OP_NAME_RANGE_OF, &[decl], &[]);
    assert_eq!(r.len(), 2);
    let (file, len) = (r[0], T_CPP.len() as u64);
    assert!(file != 0 && u32::try_from(file).is_ok());
    let span = |lo: u64, hi: u64| lo << 32 | hi;
    for op in [wire::OP_SOURCE_TEXT_OF, wire::OP_IS_EDITABLE] {
        for (f, s) in [
            (0, 0),
            (0, span(0, 3)),
            (file + 1, span(0, 1)), // a file this run was not given
            (file | 1 << 32, span(0, 1)),
            (u64::MAX, span(0, 1)),
            (file, span(4, 3)),
            (file, span(0, len + 1)),
            (file, span(len + 1, len + 1)),
            (file, u64::MAX),
        ] {
            assert_eq!(
                status(&mut h, op, &[f, s], &[]),
                errc(Errc::InvalidHandle),
                "op {op} ({f}, {s:#x})"
            );
        }
    }
    assert_eq!(bytes(&mut h, wire::OP_SOURCE_TEXT_OF, &[file, span(0, len)]), T_CPP.as_bytes());
    assert_eq!(bytes(&mut h, wire::OP_SOURCE_TEXT_OF, &[file, span(len, len)]), b"");
    assert_eq!(bytes(&mut h, wire::OP_SOURCE_TEXT_OF, &[file, r[1]]), b"foo");
}

/// §3.4: a range the subject gives out is issued a file handle once; the
/// same file has the same handle through every value.
#[test]
fn s3_4_one_file_one_handle() {
    let w = world();
    let mut h = host(&w.fake);
    let s = one(&mut h, "S");
    let main = one(&mut h, "main");
    let ds = ints(&mut h, wire::OP_DECLARATIONS_OF, &[s], &[])[0];
    let dm = ints(&mut h, wire::OP_DECLARATIONS_OF, &[main], &[])[0];
    let rs = ints(&mut h, wire::OP_NAME_RANGE_OF, &[ds], &[]);
    let rm = ints(&mut h, wire::OP_NAME_RANGE_OF, &[dm], &[]);
    assert_ne!(rs[0], rm[0], "two files");
    let foo = one(&mut h, "lib::foo");
    let df = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(ints(&mut h, wire::OP_NAME_RANGE_OF, &[df], &[])[0], rm[0], "one file");
}

/// §3.4: the host issues a handle only for a live value and gives out
/// only ranges that lie in their file; a subject that answers otherwise is
/// a host failure (`Err`), not a status.
#[test]
fn s3_4_an_inconsistent_subject_answer_is_a_host_failure() {
    // A declaration whose name range lies outside its file.
    let mut w = world();
    let d = w.fake.declarations.iter().position(|d| d.entity == w.main).unwrap();
    w.fake.declarations[d].name = Some(Range { file: w.t, lo: 0, hi: 10_000 });
    let mut h = host(&w.fake);
    let main = one(&mut h, "main");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[main], &[])[0];
    let mut out = FakeOut::with_capacity(ROOM, ROOM);
    assert!(h.call(wire::OP_NAME_RANGE_OF, &[d], &[], &mut out).is_err());
    assert_eq!(out.writes, 0);
    // A reference whose referent is not live.
    let mut w = world();
    let r = w.fake.expressions[0].call.as_ref().unwrap().reference;
    w.fake.references[r.0 as usize].referent = Entity(999);
    let mut h = host(&w.fake);
    let lib = one(&mut h, "lib");
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[lib], &[]);
    // calls_to asks the callee, which is still foo.
    let foo = one(&mut h, "lib::foo");
    let calls = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    let r = v(&mut h, wire::OP_CALLEE_REFERENCE_OF, &[calls[0]]);
    assert!(h.call(wire::OP_REFERENT_OF, &[r], &[], &mut out).is_err());
    assert_eq!(refs.len(), 1);
}

// ---------------------------------------------------------------------------
// Rule 2: convention B is count, then fill (§3.3).

/// §3.3: too little room writes nothing and returns the same counts.
#[test]
fn s3_3_convention_b_writes_all_or_nothing() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let full = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    assert_eq!(full.len(), 2);
    for cap in 0..=3 {
        let mut out = FakeOut::with_capacity(cap, 0);
        let word = h.call(wire::OP_REFERENCES_TO, &[foo], &[], &mut out).unwrap();
        assert_eq!((wire::status(word), wire::n_ints(word), wire::n_bytes(word)), (0, 2, 0));
        if cap < 2 {
            assert_eq!(out.writes, 0, "cap {cap}");
        } else {
            assert_eq!(out.ints, full, "cap {cap}");
        }
    }
    for cap in 0..=4 {
        let mut out = FakeOut::with_capacity(0, cap);
        let word = h.call(wire::OP_IDENTIFIER_OF, &[foo], &[], &mut out).unwrap();
        assert_eq!((wire::status(word), wire::n_bytes(word), wire::n_ints(word)), (0, 3, 0));
        if cap < 3 {
            assert_eq!(out.writes, 0, "cap {cap}");
        } else {
            assert_eq!(out.bytes, b"foo", "cap {cap}");
        }
    }
    // A range: two integers or none.
    let decl = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    let mut out = FakeOut::with_capacity(1, 0);
    let word = h.call(wire::OP_NAME_RANGE_OF, &[decl], &[], &mut out).unwrap();
    assert_eq!((wire::n_ints(word), out.writes), (2, 0));
    // An empty answer needs no room.
    let mut out = FakeOut::with_capacity(0, 0);
    let word = h.call(wire::OP_LOOKUP, &[], b"nothing", &mut out).unwrap();
    assert_eq!((wire::status(word), wire::n_ints(word), out.writes), (0, 0, 0));
}

// ---------------------------------------------------------------------------
// Rule 3: failures (§3.5).

/// §3.5 rules 2 and 3: failures are numbered from 1; `error_message`
/// returns each one's text and refuses any other number.
#[test]
fn s3_5_failures_are_numbered_and_have_messages() {
    let w = world();
    let mut h = host(&w.fake);
    let lib = one(&mut h, "lib");
    let w1 = call(&mut h, wire::OP_ENTITY_OF, &[lib], &[]).0; // wrong_kind
    let w2 = call(&mut h, wire::OP_KIND, &[0], &[]).0; // invalid_handle
    let w3 = call(&mut h, 1000, &[], &[]).0; // 254
    let w4 = call(&mut h, wire::OP_KIND, &[], &[]).0; // 255
    assert_eq!(
        [w1, w2, w3, w4].map(|w| (wire::status(w), wire::payload(w))),
        [(2, 1), (1, 2), (254, 3), (255, 4)]
    );
    let f = h.failures().to_vec();
    assert_eq!(
        f.iter().map(|f| (f.ordinal, f.op, f.errc)).collect::<Vec<_>>(),
        [
            (1, wire::OP_ENTITY_OF, 2),
            (2, wire::OP_KIND, 1),
            (3, 1000, 254),
            (4, wire::OP_KIND, 255)
        ]
    );
    for x in &f {
        assert!(!x.message.is_empty());
        assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[x.ordinal]), x.message.as_bytes());
    }
    // wrong_kind of a live value carries the value's place (§2.8 rule 3).
    let place = f[0].place.expect("the namespace's name");
    assert_eq!(&T_CPP.as_bytes()[place.lo as usize..place.hi as usize], b"lib");
    assert_eq!(f[1].place, None);
    // 0, the ordinal of the call itself, a later one, and one of no run.
    for n in [0, 6, 8, u64::MAX] {
        let word = call(&mut h, wire::OP_ERROR_MESSAGE, &[n], &[]).0;
        assert_eq!(wire::status(word), errc(Errc::InvalidArgument), "ordinal {n}");
    }
    // The refusals of error_message are failures too, with ordinals 5 to 8.
    assert_eq!(h.failures().len(), 8);
    assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[8]), h.failures()[7].message.as_bytes());
}

/// §3.5 rule 2 (WP-081 rule 3): a failed operation changes nothing but the
/// failure list: the handles issued afterwards and the run output are
/// those of the same run without the failures.
#[test]
fn s3_5_a_failed_operation_changes_nothing() {
    let w = world();
    let script = |h: &mut Host<'_>, noise: bool| -> Vec<u64> {
        let mut seen = Vec::new();
        let fail = |h: &mut Host<'_>, seen: &[u64]| {
            if noise {
                let any = seen.first().copied().unwrap_or(0);
                for (op, i, b) in [
                    (wire::OP_LOOKUP, vec![], b"a b".to_vec()),
                    (wire::OP_LOOKUP, vec![], vec![b'x'; 5000]),
                    (wire::OP_ENTITY_OF, vec![any], vec![]),
                    (wire::OP_PARENT_OF, vec![wire::handle(1, 40)], vec![]),
                    (wire::OP_REFERENCES_TO, vec![any, any], vec![]),
                    (wire::OP_ERROR_MESSAGE, vec![99], vec![]),
                    (wire::OP_REPLACE, vec![0, 0, 0, 0], vec![]),
                    (7, vec![], vec![]),
                ] {
                    let word = call(h, op, &i, &b).0;
                    assert_ne!(wire::status(word), 0);
                }
            }
        };
        fail(h, &seen);
        let lib = one(h, "lib");
        seen.push(lib);
        fail(h, &seen);
        let foo = one(h, "lib::foo");
        seen.push(foo);
        fail(h, &seen);
        seen.extend(ints(h, wire::OP_REFERENCES_TO, &[foo], &[]));
        fail(h, &seen);
        seen.extend(ints(h, wire::OP_CALLS_TO, &[foo], &[]));
        seen.extend(ints(h, wire::OP_PARAMETERS_OF, &[foo], &[]));
        let d = ints(h, wire::OP_DECLARATIONS_OF, &[foo], &[]);
        seen.extend(ints(h, wire::OP_NAME_RANGE_OF, &[d[0]], &[]));
        fail(h, &seen);
        seen.push(one(h, "main"));
        seen
    };
    let mut quiet = host(&w.fake);
    let mut noisy = host(&w.fake);
    assert_eq!(script(&mut quiet, false), script(&mut noisy, true));
    assert!(quiet.failures().is_empty());
    assert_eq!(noisy.failures().len(), 40);
    assert_eq!(quiet.finish(), noisy.finish());
}

// ---------------------------------------------------------------------------
// Rule 4: lookup (§6.1).

/// §6.1 rule 1: the grammar, parsed by the host.
#[test]
fn s6_1_lookup_grammar() {
    let ok: &[(&str, bool, &[&str])] = &[
        ("::", true, &[]),
        ("a", false, &["a"]),
        ("::a", true, &["a"]),
        ("a::b", false, &["a", "b"]),
        ("::_a1::B_::c", true, &["_a1", "B_", "c"]),
        // Identifiers with special meaning are identifiers.
        ("final::override::import::module", false, &["final", "override", "import", "module"]),
        ("classy::operator_", false, &["classy", "operator_"]),
    ];
    for &(text, global, components) in ok {
        let (g, c) =
            parse_qualified_name(text.as_bytes()).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(
            (g, c),
            (global, components.iter().map(|c| c.as_bytes()).collect::<Vec<_>>()),
            "{text}"
        );
    }
    let invalid: &[&[u8]] = &[
        b"",
        b":",
        b":::",
        b"::::",
        b"a::",
        b"a:::b",
        b"a::::b",
        b":a",
        b"a:b",
        b" a",
        b"a ",
        b"a:: b",
        b"1a",
        b"a::2",
        b"vector<int>",
        b"operator+",
        b"operator==",
        b"~S",
        b"a.b",
        b"a-b",
        b"a\0b",
        b"\t",
        // Keywords and alternative tokens.
        b"class",
        b"operator",
        b"and",
        b"lib::int",
        b"xor_eq::a",
        b"contract_assert",
        // Not UTF-8.
        b"\xff",
        b"a\xffb",
        b"\xc3",
        b"a::\xe9",
        // Malformed wins over undecided, across the whole text.
        "a b::\u{e9}".as_bytes(),
        "\u{e9}::a b".as_bytes(),
        "a$::1".as_bytes(),
        "1\u{e9}".as_bytes(),
        "\u{e9} x".as_bytes(),
    ];
    for &bad in invalid {
        assert_eq!(
            parse_qualified_name(bad),
            Err(Errc::InvalidArgument),
            "{:?}",
            String::from_utf8_lossy(bad)
        );
    }
    // `$` or a code point outside ASCII: version 0 does not decide.
    for undecided in
        ["caf\u{e9}", "\u{e9}::a", "a::\u{3bb}", "a$", "$", "$a::b", "a\u{a0}b", "\u{e9}"]
    {
        assert_eq!(
            parse_qualified_name(undecided.as_bytes()),
            Err(Errc::Unsupported),
            "{undecided:?}"
        );
    }
    assert_eq!(check_identifier(b"x_9"), Ok(()));
    assert_eq!(check_identifier(b"final"), Ok(()));
    assert_eq!(check_identifier(b""), Err(Errc::InvalidArgument));
    assert_eq!(check_identifier(b"and"), Err(Errc::InvalidArgument));
    assert_eq!(check_identifier(b"\xff"), Err(Errc::InvalidArgument));
    assert_eq!(check_identifier(b"a::b"), Err(Errc::InvalidArgument));
    assert_eq!(check_identifier(b"a$"), Err(Errc::Unsupported));
    assert_eq!(check_identifier("a\u{a0}b".as_bytes()), Err(Errc::Unsupported));
}

/// §6.1 rule 1: the keyword table is sorted, has no duplicate, holds the
/// alternative tokens, and leaves out the identifiers with special meaning.
#[test]
fn s6_1_keyword_table() {
    assert!(KEYWORDS.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes()), "sorted, no duplicate");
    for alt in [
        "and", "and_eq", "bitand", "bitor", "compl", "not", "not_eq", "or", "or_eq", "xor",
        "xor_eq",
    ] {
        assert!(KEYWORDS.contains(&alt), "{alt}");
    }
    for kw in [
        "class",
        "operator",
        "consteval",
        "constinit",
        "co_await",
        "char8_t",
        "concept",
        "requires",
        "contract_assert",
    ] {
        assert!(KEYWORDS.contains(&kw), "{kw}");
    }
    for id in ["final", "override", "import", "module", "pre", "post", "std"] {
        assert!(!KEYWORDS.contains(&id), "{id}");
    }
}

/// §6.1: through the host, grammar errors are statuses; components reach
/// the subject, whose walk finds by declaration through transparent
/// scopes and not into a function.
#[test]
fn s6_1_lookup_through_the_host() {
    let w = world();
    let mut h = host(&w.fake);
    for bad in ["", "a b", "lib::", "lib::foo<int>", "lib::operator+"] {
        assert_eq!(
            status(&mut h, wire::OP_LOOKUP, &[], bad.as_bytes()),
            errc(Errc::InvalidArgument),
            "{bad:?}"
        );
    }
    assert_eq!(
        status(&mut h, wire::OP_LOOKUP, &[], "lib::\u{e9}".as_bytes()),
        errc(Errc::Unsupported)
    );
    let global = one(&mut h, "::");
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[global]), 0);
    let foo = one(&mut h, "lib::foo");
    assert_eq!(one(&mut h, "::lib::foo"), foo);
    assert_eq!(one(&mut h, "lib::inl::foo"), foo, "an inline namespace is a scope too");
    let x_hidden = one(&mut h, "hidden");
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[x_hidden]), b"hidden");
    assert!(lookup(&mut h, "foo").is_empty(), "foo is not a member of the global namespace");
    assert!(lookup(&mut h, "lib::foo::a").is_empty(), "a function is not a scope");
    assert!(lookup(&mut h, "nothing").is_empty());
}

/// §7.7: a qualified name of 2^12 bytes is accepted, one more is not.
#[test]
fn s7_7_lookup_name_limit() {
    let w = world();
    let mut h = host(&w.fake);
    let at = "x".repeat(1 << 12);
    assert!(lookup(&mut h, &at).is_empty());
    let over = "x".repeat((1 << 12) + 1);
    assert_eq!(status(&mut h, wire::OP_LOOKUP, &[], over.as_bytes()), errc(Errc::LimitExceeded));
    let mut small =
        Host::new(&w.fake, Digest([0; 32]), Limits { name_bytes: 3, ..Limits::default() });
    assert_eq!(lookup(&mut small, "lib").len(), 1);
    assert_eq!(status(&mut small, wire::OP_LOOKUP, &[], b"main"), errc(Errc::LimitExceeded));
}

// ---------------------------------------------------------------------------
// Rule 5: statuses 254 and 255 (§3.5 rule 1).

/// §3.5 rule 1: an operation number the host does not have is 254.
#[test]
fn s3_5_unknown_operation_is_254() {
    let w = world();
    let mut h = host(&w.fake);
    let mut unknown: Vec<u32> =
        vec![0, 1, 24, 31, 38, 47, 52, 63, 68, 79, 85, 255, 256, 1 << 16, u32::MAX];
    unknown.extend(3..=15);
    for op in unknown {
        for (i, b) in [(vec![], vec![]), (vec![1, 2], vec![]), (vec![], b"x".to_vec())] {
            let word = call(&mut h, op, &i, &b).0;
            assert_eq!(wire::status(word), wire::STATUS_UNKNOWN_OP, "op {op}");
            assert_eq!(wire::payload(word), h.failures().len() as u64);
        }
    }
}

/// §3.5 rule 1: arguments that do not fit the operation are 255, for every
/// operation: one integer more, one fewer, text where none is taken.
#[test]
fn s3_5_a_shape_that_does_not_fit_is_255() {
    let w = world();
    let mut h = host(&w.fake);
    for o in wire::OPERATIONS {
        let mut wrong: Vec<(Vec<u64>, Vec<u8>)> = Vec::new();
        if let Arity::Exactly(n) = o.ints {
            wrong.push((vec![0; n + 1], Vec::new()));
            wrong.push((vec![0; n + 5], Vec::new()));
            if n > 0 {
                wrong.push((vec![0; n - 1], Vec::new()));
            }
        }
        if !o.text {
            let n = match o.ints {
                Arity::Exactly(n) => n,
                Arity::Any => 0,
            };
            wrong.push((vec![0; n], b"x".to_vec()));
        }
        assert!(!wrong.is_empty(), "{}", o.name);
        for (i, b) in wrong {
            assert_eq!(
                status(&mut h, o.op, &i, &b),
                wire::STATUS_MALFORMED,
                "{} with {} ints, {} bytes",
                o.name,
                i.len(),
                b.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// One test per operation of §3.2 (the queries; §6).

/// §6.2, §3.4: `kind` gives the sort bit and the kind bits of §3.4.
#[test]
fn s3_2_kind() {
    let w = world();
    let mut h = host(&w.fake);
    let lib = one(&mut h, "lib");
    assert_eq!(v(&mut h, wire::OP_KIND, &[lib]), wire::KIND_ENTITY | wire::KIND_NAMESPACE);
    let foo = one(&mut h, "lib::foo");
    assert_eq!(v(&mut h, wire::OP_KIND, &[foo]), wire::KIND_ENTITY | wire::KIND_FUNCTION);
    let s = one(&mut h, "S");
    assert_eq!(v(&mut h, wire::OP_KIND, &[s]), wire::KIND_ENTITY | wire::KIND_TYPE);
    let p = ints(&mut h, wire::OP_PARAMETERS_OF, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_KIND, &[p[0]]), wire::KIND_ENTITY | wire::KIND_PARAMETER);
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(v(&mut h, wire::OP_KIND, &[d]), wire::KIND_DECLARATION);
    let x_main = one(&mut h, "main");
    let dm = ints(&mut h, wire::OP_DECLARATIONS_OF, &[x_main], &[])[0];
    assert_eq!(v(&mut h, wire::OP_KIND, &[dm]), wire::KIND_DECLARATION | wire::KIND_DEFINITION);
    let r = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[])[0];
    assert_eq!(v(&mut h, wire::OP_KIND, &[r]), wire::KIND_REFERENCE);
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0];
    assert_eq!(v(&mut h, wire::OP_KIND, &[c]), wire::KIND_EXPRESSION | wire::KIND_CALL);
    let arg = ints(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[c], &[])[0];
    assert_eq!(v(&mut h, wire::OP_KIND, &[arg]), wire::KIND_EXPRESSION);
}

/// §6.3 rule 1: `identifier_of`.
#[test]
fn s3_2_identifier_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[foo]), b"foo");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[d]), b"foo");
    let r = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[])[0];
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[r]), b"foo");
    let global = one(&mut h, "::");
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[global]), b"");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0];
    assert_eq!(status(&mut h, wire::OP_IDENTIFIER_OF, &[c], &[]), errc(Errc::WrongKind));
}

/// §6.3 rule 2: `display_string_of` accepts every live info.
#[test]
fn s3_2_display_string_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    let r = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[])[0];
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0];
    for x in [foo, d, r, c] {
        assert!(!bytes(&mut h, wire::OP_DISPLAY_STRING_OF, &[x]).is_empty());
    }
    assert_eq!(status(&mut h, wire::OP_DISPLAY_STRING_OF, &[0], &[]), errc(Errc::InvalidHandle));
}

/// §6.4 rule 1: `parent_of`.
#[test]
fn s3_2_parent_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let inl = one(&mut h, "lib::inl");
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[foo]), inl);
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[inl]), one(&mut h, "lib"));
    let p = ints(&mut h, wire::OP_PARAMETERS_OF, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[p[0]]), foo);
    let global = one(&mut h, "::");
    assert_eq!(v(&mut h, wire::OP_PARENT_OF, &[global]), 0);
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(status(&mut h, wire::OP_PARENT_OF, &[d], &[]), errc(Errc::WrongKind));
}

/// §6.4 rule 2: `parameters_of`.
#[test]
fn s3_2_parameters_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let p = ints(&mut h, wire::OP_PARAMETERS_OF, &[foo], &[]);
    assert_eq!(p.len(), 2);
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[p[0]]), b"a");
    assert_eq!(bytes(&mut h, wire::OP_IDENTIFIER_OF, &[p[1]]), b"b");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(ints(&mut h, wire::OP_PARAMETERS_OF, &[d], &[]), p);
    let x_main = one(&mut h, "main");
    assert!(ints(&mut h, wire::OP_PARAMETERS_OF, &[x_main], &[]).is_empty());
    let lib = one(&mut h, "lib");
    let r = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[])[0];
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0];
    for x in [lib, r, c] {
        assert_eq!(status(&mut h, wire::OP_PARAMETERS_OF, &[x], &[]), errc(Errc::WrongKind));
    }
}

/// §6.4 rule 3: `declarations_of`.
#[test]
fn s3_2_declarations_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[]);
    assert_eq!(d.len(), 1);
    let global = one(&mut h, "::");
    assert!(ints(&mut h, wire::OP_DECLARATIONS_OF, &[global], &[]).is_empty());
    let p = ints(&mut h, wire::OP_PARAMETERS_OF, &[foo], &[]);
    assert_eq!(status(&mut h, wire::OP_DECLARATIONS_OF, &[p[0]], &[]), errc(Errc::Unsupported));
    assert_eq!(status(&mut h, wire::OP_DECLARATIONS_OF, &[d[0]], &[]), errc(Errc::WrongKind));
}

/// §6.4 rule 4: `entity_of`.
#[test]
fn s3_2_entity_of() {
    let w = world();
    let mut h = host(&w.fake);
    let main = one(&mut h, "main");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[main], &[])[0];
    assert_eq!(v(&mut h, wire::OP_ENTITY_OF, &[d]), main);
    assert_eq!(status(&mut h, wire::OP_ENTITY_OF, &[main], &[]), errc(Errc::WrongKind));
}

/// §6.5 rule 1, §5.5: `references_to` matches by origin.
#[test]
fn s3_2_references_to() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    assert_eq!(refs.len(), 2, "the reference to the specialisation is a reference to foo");
    let lib = one(&mut h, "lib");
    assert_eq!(ints(&mut h, wire::OP_REFERENCES_TO, &[lib], &[]).len(), 1);
    assert_eq!(status(&mut h, wire::OP_REFERENCES_TO, &[refs[0]], &[]), errc(Errc::WrongKind));
}

/// §6.5 rule 2, §5.5: `referent_of` is the entity as resolved.
#[test]
fn s3_2_referent_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_REFERENT_OF, &[refs[0]]), foo);
    let spec = v(&mut h, wire::OP_REFERENT_OF, &[refs[1]]);
    assert_ne!(spec, foo, "a specialisation is its own entity");
    assert_eq!(v(&mut h, wire::OP_KIND, &[spec]), wire::KIND_ENTITY | wire::KIND_FUNCTION);
    assert_eq!(status(&mut h, wire::OP_REFERENT_OF, &[foo], &[]), errc(Errc::WrongKind));
}

fn text_of(h: &mut Host<'_>, r: &[u64]) -> Vec<u8> {
    bytes(h, wire::OP_SOURCE_TEXT_OF, r)
}

/// §6.6 rule 1: `name_range_of`: a range or nothing.
#[test]
fn s3_2_name_range_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    let r = ints(&mut h, wire::OP_NAME_RANGE_OF, &[d], &[]);
    assert_eq!(text_of(&mut h, &r), b"foo");
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    let rr = ints(&mut h, wire::OP_NAME_RANGE_OF, &[refs[1]], &[]);
    assert_eq!(text_of(&mut h, &rr), b"foo");
    assert_ne!(rr, r);
    let anon = lookup(&mut h, "hidden");
    let anon_ns = v(&mut h, wire::OP_PARENT_OF, &[anon[0]]);
    let da = ints(&mut h, wire::OP_DECLARATIONS_OF, &[anon_ns], &[])[0];
    assert!(ints(&mut h, wire::OP_NAME_RANGE_OF, &[da], &[]).is_empty(), "no name: nothing");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0];
    for x in [foo, c] {
        assert_eq!(status(&mut h, wire::OP_NAME_RANGE_OF, &[x], &[]), errc(Errc::WrongKind));
    }
}

/// §6.6 rules 2 to 4: `extent_of`.
#[test]
fn s3_2_extent_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    let e = ints(&mut h, wire::OP_EXTENT_OF, &[refs[0]], &[]);
    assert_eq!(text_of(&mut h, &e), b"lib::foo");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    let e = ints(&mut h, wire::OP_EXTENT_OF, &[c[1]], &[]);
    assert_eq!(text_of(&mut h, &e), b"lib::foo(3, 4)");
    assert_eq!(status(&mut h, wire::OP_EXTENT_OF, &[foo], &[]), errc(Errc::WrongKind));
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[foo], &[])[0];
    assert_eq!(status(&mut h, wire::OP_EXTENT_OF, &[d], &[]), errc(Errc::Unsupported));
    let failure = h.failures().last().unwrap().clone();
    assert!(failure.place.is_some(), "the declaration's place");
}

/// §6.6 rule 5: `source_text_of` (ranges validated: `s3_4_ranges_are_validated`).
#[test]
fn s3_2_source_text_of() {
    let w = world();
    let mut h = host(&w.fake);
    let s = one(&mut h, "S");
    let d = ints(&mut h, wire::OP_DECLARATIONS_OF, &[s], &[])[0];
    let r = ints(&mut h, wire::OP_NAME_RANGE_OF, &[d], &[]);
    assert_eq!(text_of(&mut h, &r), b"S");
    assert_eq!(text_of(&mut h, &[r[0], H_H.len() as u64]), H_H.as_bytes());
    assert_eq!(status(&mut h, wire::OP_SOURCE_TEXT_OF, &[0, 0], &[]), errc(Errc::InvalidHandle));
}

/// §6.6 rule 6: `is_editable` is the file's ownership.
#[test]
fn s3_2_is_editable() {
    let w = world();
    let mut h = host(&w.fake);
    let range_of = |h: &mut Host<'_>, name: &str| {
        let e = one(h, name);
        let d = ints(h, wire::OP_DECLARATIONS_OF, &[e], &[])[0];
        ints(h, wire::OP_NAME_RANGE_OF, &[d], &[])
    };
    let in_t = range_of(&mut h, "main");
    let in_h = range_of(&mut h, "S");
    assert_eq!(v(&mut h, wire::OP_IS_EDITABLE, &in_t), 1);
    assert_eq!(v(&mut h, wire::OP_IS_EDITABLE, &in_h), 0);
    assert_eq!(
        v(&mut h, wire::OP_IS_EDITABLE, &[in_t[0], 0]),
        1,
        "an empty range of an owned file"
    );
    assert_eq!(status(&mut h, wire::OP_IS_EDITABLE, &[0, 0], &[]), errc(Errc::InvalidHandle));
}

/// §6.7 rule 3: `calls_to` matches by origin.
#[test]
fn s3_2_calls_to() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    assert_eq!(c.len(), 2);
    let x_main = one(&mut h, "main");
    assert!(ints(&mut h, wire::OP_CALLS_TO, &[x_main], &[]).is_empty());
    let lib = one(&mut h, "lib");
    assert_eq!(status(&mut h, wire::OP_CALLS_TO, &[lib], &[]), errc(Errc::WrongKind));
    assert_eq!(status(&mut h, wire::OP_CALLS_TO, &[c[0]], &[]), errc(Errc::WrongKind));
}

/// §6.7 rules 4 and 6: `callee_of`.
#[test]
fn s3_2_callee_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_CALLEE_OF, &[c[0]]), foo);
    assert_ne!(v(&mut h, wire::OP_CALLEE_OF, &[c[1]]), foo, "the specialisation, as resolved");
    let arg = ints(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[c[0]], &[])[0];
    assert_eq!(status(&mut h, wire::OP_CALLEE_OF, &[arg], &[]), errc(Errc::WrongKind));
    assert_eq!(status(&mut h, wire::OP_CALLEE_OF, &[foo], &[]), errc(Errc::WrongKind));
}

/// §6.7 rules 4 and 6: `callee_reference_of`.
#[test]
fn s3_2_callee_reference_of() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    assert_eq!(v(&mut h, wire::OP_CALLEE_REFERENCE_OF, &[c[0]]), refs[0]);
    assert_eq!(v(&mut h, wire::OP_CALLEE_REFERENCE_OF, &[c[1]]), refs[1]);
    let arg = ints(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[c[0]], &[])[0];
    assert_eq!(status(&mut h, wire::OP_CALLEE_REFERENCE_OF, &[arg], &[]), errc(Errc::WrongKind));
    assert_eq!(
        status(&mut h, wire::OP_CALLEE_REFERENCE_OF, &[refs[0]], &[]),
        errc(Errc::WrongKind)
    );
}

/// §6.7 rules 5 and 6: `arguments_as_written`.
#[test]
fn s3_2_arguments_as_written() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    let c = ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
    let args = ints(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[c[1]], &[]);
    let texts: Vec<Vec<u8>> = args
        .iter()
        .map(|&a| {
            let e = ints(&mut h, wire::OP_EXTENT_OF, &[a], &[]);
            text_of(&mut h, &e)
        })
        .collect();
    assert_eq!(texts, [b"3".to_vec(), b"4".to_vec()]);
    assert_eq!(
        status(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[args[0]], &[]),
        errc(Errc::WrongKind)
    );
    assert_eq!(status(&mut h, wire::OP_ARGUMENTS_AS_WRITTEN, &[foo], &[]), errc(Errc::WrongKind));
}

/// §3.2, operations 64 to 84: dispatched, shape-checked, and `unsupported`
/// until the effects land (WP-088).
#[test]
fn s3_2_effects_are_unsupported_for_now() {
    let w = world();
    let mut h = host(&w.fake);
    for o in wire::OPERATIONS.iter().filter(|o| o.effect) {
        let n = match o.ints {
            Arity::Exactly(n) => n,
            Arity::Any => 3,
        };
        let text: &[u8] = if o.text { b"t" } else { b"" };
        assert_eq!(status(&mut h, o.op, &vec![0; n], text), errc(Errc::Unsupported), "{}", o.name);
        if o.text {
            assert_eq!(
                status(&mut h, o.op, &vec![0; n], &[]),
                errc(Errc::Unsupported),
                "{}",
                o.name
            );
        }
    }
    let out = h.finish();
    assert!(out.edits.is_empty() && out.reports.is_empty());
}

// ---------------------------------------------------------------------------
// Rule 6: the canonical form (§7.6).

fn place(path: &str, lo: u32, hi: u32) -> Place {
    Place {
        file: FileRef { path: path.as_bytes().to_vec(), size: 100, content: Digest([3; 32]) },
        lo,
        hi,
    }
}

fn edit(path: &str, lo: u32, hi: u32, pieces: Vec<Piece>, notes: &[&str]) -> Edit {
    Edit {
        place: place(path, lo, hi),
        pieces,
        tier: 0,
        notes: notes.iter().map(|n| n.as_bytes().to_vec()).collect(),
    }
}

fn lit(s: &str) -> Piece {
    Piece::Literal(s.as_bytes().to_vec())
}

fn rule(name: &str, to: &str, parent: Parent) -> Rule {
    Rule {
        parent,
        kind: KeyKind::Function,
        name: Name::Identifier(name.as_bytes().to_vec()),
        disc: None,
        to_parent: parent,
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    }
}

/// A raw output with every kind of disorder §7.6 removes.
fn raw() -> RunOutput {
    let mut out = RunOutput::empty(Digest([9; 32]));
    out.edits = vec![
        edit("b", 5, 9, vec![lit("x")], &["n2"]), // 0
        edit("a", 5, 9, vec![lit("x")], &[]),     // 1
        edit("b", 5, 9, vec![lit("x"), lit(""), Piece::Literal(vec![])], &["n1", "n2"]), // 2 = 0
        edit("a", 1, 3, vec![lit("p"), lit("q")], &[]), // 3
        edit("a", 1, 3, vec![lit("pq")], &["z"]), // 4 = 3
        edit("a", 1, 3, vec![Piece::Copy(place("a", 10, 20))], &[]), // 5
        edit("a", 1, 3, vec![Piece::Spelled { key: Digest([1; 32]), text: b"::f".to_vec() }], &[]), // 6
        edit("c", 0, 1, vec![], &[]), // 7
    ];
    out.edits[2].tier = 1;
    out.groups = vec![
        vec![0, 2],    // one edit twice: a group of one
        vec![],        // a group never used
        vec![1, 3],    // a, then merged with the next through 3 = 4
        vec![4, 5, 5], // shares 3 = 4
        vec![7],       // one member
    ];
    let report = |p: Option<Place>, s, m: &str| Report {
        place: p,
        severity: s,
        message: m.as_bytes().to_vec(),
        attached: false,
    };
    out.reports = vec![
        report(Some(place("b", 0, 1)), Severity::Note, "m"),
        report(None, Severity::Error, "z"),
        report(Some(place("a", 0, 1)), Severity::Review, "m"),
        report(Some(place("a", 0, 1)), Severity::Note, "m"),
        report(Some(place("b", 0, 1)), Severity::Note, "m"),
    ];
    let changed = |k: u8, p: Place, r: &str| Changed {
        key: Digest([k; 32]),
        place: p,
        reason: r.as_bytes().to_vec(),
    };
    out.changed = vec![
        changed(2, place("a", 0, 1), "r"),
        changed(1, place("b", 0, 1), "r"),
        changed(2, place("a", 0, 1), "r"),
        changed(1, place("a", 0, 1), "s"),
    ];
    out.renamed = vec![
        rule("g", "h", Parent::Root),
        rule("f", "f2", Parent::Unit(Digest([5; 32]))),
        rule("f", "f2", Parent::Root),
        rule("g", "h", Parent::Root),
    ];
    out.counters = vec![
        Counter { name: "b.x".to_owned(), value: 1 },
        Counter { name: "a.x".to_owned(), value: 2 },
    ];
    out
}

/// §7.6 rules 1 and 2: edits sorted by (file, lo, hi, pieces) and joined by
/// content (notes unioned); groups rewritten, merged and pruned; reports,
/// declarations and rules sorted and deduplicated.
#[test]
fn s7_6_canonical_form() {
    let c = raw().canonical();
    let keys: Vec<(&[u8], u32, u32, &[Piece])> = c
        .edits
        .iter()
        .map(|e| (&e.place.file.path[..], e.place.lo, e.place.hi, &e.pieces[..]))
        .collect();
    assert_eq!(
        keys,
        vec![
            (&b"a"[..], 1, 3, &[lit("pq")][..]),
            (&b"a"[..], 1, 3, &[Piece::Copy(place("a", 10, 20))][..]),
            (
                &b"a"[..],
                1,
                3,
                &[Piece::Spelled { key: Digest([1; 32]), text: b"::f".to_vec() }][..]
            ),
            (&b"a"[..], 5, 9, &[lit("x")][..]),
            (&b"b"[..], 5, 9, &[lit("x")][..]),
            (&b"c"[..], 0, 1, &[][..]),
        ]
    );
    assert_eq!(c.edits[0].notes, vec![b"z".to_vec()]);
    assert_eq!(c.edits[4].notes, vec![b"n1".to_vec(), b"n2".to_vec()]);
    assert_eq!(c.edits[4].tier, 1, "joined edits keep the higher tier");
    // raw 1 -> 3, raw 3/4 -> 0, raw 5 -> 1: one group {0, 1, 3}.
    assert_eq!(c.groups, vec![vec![0, 1, 3]]);
    assert_eq!(c.reports.len(), 4);
    assert!(c.reports.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(c.reports[0].place, None);
    assert_eq!(c.changed.len(), 3);
    assert!(c.changed.windows(2).all(|w| w[0] < w[1]));
    let table = RuleTable::new(c.renamed.clone()).expect("a rule table");
    assert_eq!(table.rules(), &c.renamed[..], "sorted as a rule table");
    assert_eq!(c.renamed.len(), 3);
    assert_eq!(c.counters.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a.x", "b.x"]);
    assert!(c.is_canonical());
    assert!(!raw().is_canonical());
    assert_eq!(c.clone().canonical(), c, "idempotent");
}

/// §7.6 rule 1: nothing depends on the order of the requests.
#[test]
fn s7_6_canonical_form_is_independent_of_order() {
    let base = raw();
    let expected = base.clone().canonical();
    let mut rng = Rng(0x5eed);
    for _ in 0..200 {
        let mut r = base.clone();
        // Permute edits and rewrite the groups to the new positions.
        let n = r.edits.len();
        let mut perm: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            perm.swap(i, rng.below(i as u64 + 1) as usize);
        }
        let mut new_pos = vec![0; n];
        let mut edits = Vec::with_capacity(n);
        for (to, &from) in perm.iter().enumerate() {
            new_pos[from] = to;
            edits.push(r.edits[from].clone());
        }
        r.edits = edits;
        for g in &mut r.groups {
            for m in g.iter_mut() {
                *m = new_pos[*m];
            }
            g.reverse();
        }
        r.groups.reverse();
        r.reports.reverse();
        r.changed.rotate_left(1);
        r.renamed.rotate_left(2);
        r.counters.reverse();
        assert_eq!(r.canonical(), expected);
    }
}

/// §7.6: `finish` is canonical and carries the host's counters, sorted.
#[test]
fn s7_6_finish_is_canonical_with_counters() {
    let w = world();
    let mut h = host(&w.fake);
    let foo = one(&mut h, "lib::foo");
    one(&mut h, "main");
    ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
    let _ = status(&mut h, wire::OP_ENTITY_OF, &[foo], &[]); // failed: not counted
    let out = h.finish();
    assert!(out.is_canonical());
    assert_eq!(out.transform, Digest([1; 32]));
    let counters: Vec<(&str, u64)> =
        out.counters.iter().map(|c| (c.name.as_str(), c.value)).collect();
    assert_eq!(
        counters,
        [
            ("api.edits", 0),
            ("api.op.lookup", 2),
            ("api.op.references_to", 1),
            ("api.reports", 0),
            ("api.unresolved_spelling_matches", 0)
        ]
    );
}

// ---------------------------------------------------------------------------
// The property test (§3.5 rules 1, 4 and 5).

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// §3.5 rules 1 and 4: random operation numbers, integers and bytes get a
/// status (success, an `errc`, 254 or 255) and never a host failure or a
/// panic; a failure's payload is its ordinal; writes stay within capacity.
#[test]
fn s3_5_random_calls_get_a_status_and_never_fail_the_host() {
    let w = world();
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let names: [&[u8]; 8] =
        [b"lib", b"lib::foo", b"::", b"main", b"S", b"hidden", b"lib::inl", b"x y"];
    for round in 0..20 {
        let mut h = host(&w.fake);
        let mut pool: Vec<u64> = vec![0, 1, u64::MAX];
        for _ in 0..2_000 {
            let op = match rng.below(4) {
                0 => rng.next() as u32 % 128,
                1 => rng.next() as u32,
                _ => wire::OPERATIONS[rng.below(wire::OPERATIONS.len() as u64) as usize].op,
            };
            let n_in = match rng.below(5) {
                0 => rng.below(8) as usize,
                _ => wire::operation(op).map_or(1, |o| match o.ints {
                    Arity::Exactly(n) => n,
                    Arity::Any => rng.below(4) as usize,
                }),
            };
            let ints: Vec<u64> = (0..n_in)
                .map(|_| match rng.below(4) {
                    0 => rng.next(),
                    1 => rng.below(64) << 32 | rng.below(64),
                    _ => pool[rng.below(pool.len() as u64) as usize],
                })
                .collect();
            let bytes: Vec<u8> = match rng.below(4) {
                0 => (0..rng.below(12)).map(|_| rng.next() as u8).collect(),
                1 => names[rng.below(names.len() as u64) as usize].to_vec(),
                _ => Vec::new(),
            };
            let mut out = FakeOut::with_capacity(rng.below(6) as usize, rng.below(80) as usize);
            let before = h.failures().len() as u64;
            let word = h.call(op, &ints, &bytes, &mut out).unwrap_or_else(|e| {
                panic!("round {round}: op {op} {ints:x?} {bytes:?}: host failure {e}")
            });
            let s = wire::status(word);
            assert!(matches!(s, 0..=6 | 254 | 255), "status {s}");
            if s == 0 {
                assert_eq!(h.failures().len() as u64, before);
                pool.extend(&out.ints);
                if let Some(o) = wire::operation(op)
                    && o.convention == Convention::V
                {
                    pool.push(wire::payload(word));
                }
            } else {
                assert_eq!(wire::payload(word), before + 1);
                assert_eq!(h.failures().len() as u64, before + 1);
                assert_eq!(out.writes, 0, "a failure writes nothing");
            }
        }
        assert!(h.finish().is_canonical());
    }
}

// ---------------------------------------------------------------------------
// The fix round of the WP-081 review.

/// §3.5 rule 3: the text of the first `kept_failures` failures and of the
/// last is kept; `error_message` of another ordinal of the run succeeds
/// with the fixed text; an ordinal never issued is refused.
#[test]
fn s3_5_failure_records_are_bounded() {
    let w = world();
    let mut h =
        Host::new(&w.fake, Digest([1; 32]), Limits { kept_failures: 3, ..Limits::default() });
    for _ in 0..10 {
        assert_eq!(status(&mut h, wire::OP_KIND, &[0], &[]), errc(Errc::InvalidHandle));
    }
    assert_eq!(h.failure_count(), 10);
    assert_eq!(h.failures().iter().map(|f| f.ordinal).collect::<Vec<_>>(), [1, 2, 3]);
    let last = h.last_failure().expect("ten failures").clone();
    assert_eq!(last.ordinal, 10);
    let fixed = |n: u64| {
        format!(
            "the text of failure {n} was not kept: a run keeps the text of its first 3 failures \
             and of its last"
        )
    };
    // A kept ordinal, a dropped one, the last.
    let kept = h.failures()[1].message.clone();
    assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[2]), kept.as_bytes());
    assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[5]), fixed(5).as_bytes());
    assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[10]), last.message.as_bytes());
    assert_eq!(h.failure_count(), 10, "a successful error_message is no failure");
    // Never issued: the call's own ordinal and a later one.
    assert_eq!(status(&mut h, wire::OP_ERROR_MESSAGE, &[11], &[]), errc(Errc::InvalidArgument));
    assert_eq!(status(&mut h, wire::OP_ERROR_MESSAGE, &[99], &[]), errc(Errc::InvalidArgument));
    assert_eq!(h.failure_count(), 12);
    assert_eq!(h.last_failure().map(|f| f.ordinal), Some(12));
    // 10 was the last; it is now an ordinary dropped one.
    assert_eq!(bytes(&mut h, wire::OP_ERROR_MESSAGE, &[10]), fixed(10).as_bytes());
    assert_eq!(h.failures().len(), 3);
    // With the default bound the text is the specification's.
    let mut h = host(&w.fake);
    for _ in 0..(1 << 16) + 2 {
        let _ = status(&mut h, wire::OP_KIND, &[0], &[]);
    }
    assert_eq!(
        bytes(&mut h, wire::OP_ERROR_MESSAGE, &[(1 << 16) + 1]),
        b"the text of failure 65537 was not kept: a run keeps the text of its first 65536 \
          failures and of its last"
    );
}

/// §3.5 rule 3: memory does not grow with the number of failures.
#[test]
fn s3_5_failure_memory_is_bounded() {
    let w = world();
    let mut h =
        Host::new(&w.fake, Digest([1; 32]), Limits { kept_failures: 1000, ..Limits::default() });
    let mut out = FakeOut::with_capacity(0, 0);
    let mut at_bound = 0;
    for i in 1..=1_000_000_u64 {
        let word = h.call(wire::OP_KIND, &[0], &[], &mut out).expect("no host failure");
        assert_eq!(wire::payload(word), i);
        if i == 1001 {
            at_bound = h.failure_bytes();
        }
    }
    assert_eq!(h.failure_count(), 1_000_000);
    assert_eq!(h.failures().len(), 1000);
    assert!(at_bound > 0);
    assert_eq!(h.failure_bytes(), at_bound, "the stored bytes stop growing at the bound");
}

/// A subject that delegates to the fake but for one lie.
#[derive(Clone, Copy, Debug)]
enum Lie {
    Declarations(Errc),
    CallsTo(Errc),
    NameRange(Errc),
    Extent(Errc),
    IdentifierNone,
    ParametersNone,
    CalleeNone,
    CalleeReferenceNone,
    ArgumentsNone,
}

struct Liar<'a> {
    fake: &'a FakeSubject,
    lie: Lie,
}

impl Subject for Liar<'_> {
    fn is_live(&self, v: Value) -> bool {
        self.fake.is_live(v)
    }
    fn file_len(&self, f: File) -> Option<u32> {
        self.fake.file_len(f)
    }
    fn kind_bits(&self, v: Value) -> u64 {
        self.fake.kind_bits(v)
    }
    fn lookup(&self, global: bool, components: &[&[u8]]) -> Vec<Entity> {
        self.fake.lookup(global, components)
    }
    fn identifier(&self, v: Value) -> Option<Vec<u8>> {
        match (self.lie, v) {
            (Lie::IdentifierNone, Value::Entity(_)) => None,
            _ => self.fake.identifier(v),
        }
    }
    fn display(&self, v: Value) -> String {
        self.fake.display(v)
    }
    fn parent(&self, e: Entity) -> Option<Entity> {
        self.fake.parent(e)
    }
    fn parameters(&self, v: Value) -> Option<Vec<Entity>> {
        if let Lie::ParametersNone = self.lie { None } else { self.fake.parameters(v) }
    }
    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc> {
        if let Lie::Declarations(x) = self.lie { Err(x) } else { self.fake.declarations(e) }
    }
    fn entity_of(&self, d: Declaration) -> Entity {
        self.fake.entity_of(d)
    }
    fn origin(&self, e: Entity) -> Entity {
        self.fake.origin(e)
    }
    fn references_to(&self, e: Entity) -> Vec<Reference> {
        self.fake.references_to(e)
    }
    fn referent(&self, r: Reference) -> Entity {
        self.fake.referent(r)
    }
    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc> {
        if let Lie::NameRange(x) = self.lie { Err(x) } else { self.fake.name_range(v) }
    }
    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        if let Lie::Extent(x) = self.lie { Err(x) } else { self.fake.extent(v) }
    }
    fn text(&self, r: Range) -> &[u8] {
        self.fake.text(r)
    }
    fn owned(&self, f: File) -> bool {
        self.fake.owned(f)
    }
    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc> {
        if let Lie::CallsTo(x) = self.lie { Err(x) } else { self.fake.calls_to(e) }
    }
    fn callee(&self, x: Expression) -> Option<Entity> {
        if let Lie::CalleeNone = self.lie { None } else { self.fake.callee(x) }
    }
    fn callee_reference(&self, x: Expression) -> Option<Reference> {
        if let Lie::CalleeReferenceNone = self.lie { None } else { self.fake.callee_reference(x) }
    }
    fn arguments(&self, x: Expression) -> Option<Vec<Expression>> {
        if let Lie::ArgumentsNone = self.lie { None } else { self.fake.arguments(x) }
    }
    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64> {
        self.fake.unresolved_named(identifier)
    }
    fn tier(&self, r: Range) -> Option<u8> {
        self.fake.tier(r)
    }
    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        self.fake.qualified(e)
    }
    fn references_at(&self, r: Range) -> Vec<Reference> {
        self.fake.references_at(r)
    }
    fn found_directly(&self, r: Reference) -> bool {
        self.fake.found_directly(r)
    }
    fn entity_key(&self, e: Entity) -> Option<Digest> {
        self.fake.entity_key(e)
    }
    fn unit_keys(&self, v: Value) -> Result<Vec<Digest>, Errc> {
        self.fake.unit_keys(v)
    }
    fn rename_rule(&self, e: Entity, new_identifier: &[u8]) -> Result<Rule, Errc> {
        self.fake.rename_rule(e, new_identifier)
    }
    fn place(&self, v: Value) -> Option<(Range, bool)> {
        self.fake.place(v)
    }
    fn file_ref(&self, f: File) -> FileRef {
        self.fake.file_ref(f)
    }
}

/// What one call gives under a lie: `Ok(status)` or the host failure.
fn under(fake: &FakeSubject, lie: Lie, op: u32, arg: &str) -> Result<u8, String> {
    // The handles come from an honest host: numbering is the subject's, so
    // a second host issues the same handles in the same order.
    let liar = Liar { fake, lie };
    let mut h = Host::new(&liar, Digest([1; 32]), Limits::default());
    let foo = one(&mut h, "lib::foo");
    let a = match arg {
        "foo" => foo,
        "decl" => {
            let main = one(&mut h, "main");
            ints(&mut h, wire::OP_DECLARATIONS_OF, &[main], &[])[0]
        }
        "ref" => ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[])[0],
        "call" => ints(&mut h, wire::OP_CALLS_TO, &[foo], &[])[0],
        _ => unreachable!("{arg}"),
    };
    let mut out = FakeOut::with_capacity(ROOM, ROOM);
    let word = h.call(op, &[a], &[], &mut out)?;
    assert_eq!(out.writes, 0, "a refusal or a failure writes nothing");
    Ok(wire::status(word))
}

/// §6, the trait documentation: a status the specification allows the
/// subject is passed on; any other is a host failure naming the method and
/// the status.
#[test]
fn s6_subject_statuses_outside_the_specification_are_host_failures() {
    let w = world();
    let f = &w.fake;
    let failure = |r: Result<u8, String>, method: &str, status: &str| {
        let m = r.expect_err("a host failure");
        assert!(m.contains(method) && m.contains(status), "{m}");
    };
    // declarations: unsupported allowed; anything else not.
    assert_eq!(
        under(f, Lie::Declarations(Errc::Unsupported), wire::OP_DECLARATIONS_OF, "foo"),
        Ok(errc(Errc::Unsupported))
    );
    failure(
        under(f, Lie::Declarations(Errc::NotEditable), wire::OP_DECLARATIONS_OF, "foo"),
        "Subject::declarations",
        "not_editable",
    );
    failure(
        under(f, Lie::Declarations(Errc::WrongKind), wire::OP_DECLARATIONS_OF, "foo"),
        "Subject::declarations",
        "wrong_kind",
    );
    // calls_to: wrong_kind allowed; anything else not.
    assert_eq!(
        under(f, Lie::CallsTo(Errc::WrongKind), wire::OP_CALLS_TO, "foo"),
        Ok(errc(Errc::WrongKind))
    );
    failure(
        under(f, Lie::CallsTo(Errc::Unsupported), wire::OP_CALLS_TO, "foo"),
        "Subject::calls_to",
        "unsupported",
    );
    // name_range and extent: no Err at all.
    for e in [Errc::InvalidHandle, Errc::WrongKind, Errc::Unsupported, Errc::LimitExceeded] {
        failure(
            under(f, Lie::NameRange(e), wire::OP_NAME_RANGE_OF, "decl"),
            "Subject::name_range",
            e.name(),
        );
        failure(under(f, Lie::Extent(e), wire::OP_EXTENT_OF, "ref"), "Subject::extent", e.name());
    }
    // identifier: None for an entity is a host failure, from identifier_of
    // and from the two operations that record the identifier (§6.5 rule 6).
    failure(
        under(f, Lie::IdentifierNone, wire::OP_IDENTIFIER_OF, "foo"),
        "Subject::identifier",
        "None",
    );
    failure(
        under(f, Lie::IdentifierNone, wire::OP_REFERENCES_TO, "foo"),
        "Subject::identifier",
        "None",
    );
    failure(under(f, Lie::IdentifierNone, wire::OP_CALLS_TO, "foo"), "Subject::identifier", "None");
    // The Option methods whose None is wrong_kind.
    let wrong = errc(Errc::WrongKind);
    assert_eq!(under(f, Lie::ParametersNone, wire::OP_PARAMETERS_OF, "foo"), Ok(wrong));
    assert_eq!(under(f, Lie::CalleeNone, wire::OP_CALLEE_OF, "call"), Ok(wrong));
    assert_eq!(under(f, Lie::CalleeReferenceNone, wire::OP_CALLEE_REFERENCE_OF, "call"), Ok(wrong));
    assert_eq!(under(f, Lie::ArgumentsNone, wire::OP_ARGUMENTS_AS_WRITTEN, "call"), Ok(wrong));
}

fn unresolved_counter(out: &RunOutput) -> Option<u64> {
    out.counters.iter().find(|c| c.name == "api.unresolved_spelling_matches").map(|c| c.value)
}

/// §6.5 rule 6: `api.unresolved_spelling_matches` sums, once per
/// identifier, the subject's answer for every non-empty identifier given
/// to a successful `references_to` or `calls_to`.
#[test]
fn s6_5_unresolved_spelling_matches() {
    let mut w = world();
    w.fake.unresolved =
        Some([(b"foo".to_vec(), 3), (b"lib".to_vec(), 5), (b"main".to_vec(), 7)].into());
    let run = |f: &FakeSubject, script: &dyn Fn(&mut Host<'_>)| {
        let mut h = host(f);
        script(&mut h);
        unresolved_counter(&h.finish())
    };
    // No identifier: present and 0.
    assert_eq!(run(&w.fake, &|_| {}), Some(0));
    // Two overloads (foo and its specialisation) of one name count once.
    let overloads = |h: &mut Host<'_>| {
        let foo = one(h, "lib::foo");
        let calls = ints(h, wire::OP_CALLS_TO, &[foo], &[]);
        let mut foos: Vec<u64> = calls.iter().map(|&c| v(h, wire::OP_CALLEE_OF, &[c])).collect();
        foos.dedup();
        assert_eq!(foos.len(), 2, "foo and its specialisation");
        for x in foos {
            ints(h, wire::OP_REFERENCES_TO, &[x], &[]);
        }
    };
    assert_eq!(run(&w.fake, &overloads), Some(3));
    // Twice, or in another order, changes nothing.
    let forward = |h: &mut Host<'_>| {
        let foo = one(h, "lib::foo");
        let main = one(h, "main");
        ints(h, wire::OP_REFERENCES_TO, &[foo], &[]);
        ints(h, wire::OP_CALLS_TO, &[main], &[]);
    };
    let backward = |h: &mut Host<'_>| {
        let main = one(h, "main");
        let foo = one(h, "lib::foo");
        for _ in 0..2 {
            ints(h, wire::OP_CALLS_TO, &[main], &[]);
            ints(h, wire::OP_CALLS_TO, &[foo], &[]);
            ints(h, wire::OP_REFERENCES_TO, &[foo], &[]);
        }
    };
    assert_eq!(run(&w.fake, &forward), Some(10));
    assert_eq!(run(&w.fake, &backward), Some(10));
    // A failed references_to or calls_to adds nothing.
    let failed = |h: &mut Host<'_>| {
        let lib = one(h, "lib");
        assert_eq!(status(h, wire::OP_REFERENCES_TO, &[lib, lib], &[]), 255);
        assert_eq!(status(h, wire::OP_CALLS_TO, &[lib], &[]), errc(Errc::WrongKind));
    };
    assert_eq!(run(&w.fake, &failed), Some(0));
    // A subject that cannot tell: the counter is absent.
    w.fake.unresolved = None;
    assert_eq!(run(&w.fake, &forward), None);
    assert_eq!(run(&w.fake, &|_| {}), Some(0), "no question asked");
}

/// §7.6 rule 6: the host writes every counter itself, each name once.
#[test]
fn s7_6_counter_names_are_unique() {
    let w = world();
    let mut h = host(&w.fake);
    for _ in 0..3 {
        let foo = one(&mut h, "lib::foo");
        ints(&mut h, wire::OP_REFERENCES_TO, &[foo], &[]);
        ints(&mut h, wire::OP_CALLS_TO, &[foo], &[]);
        v(&mut h, wire::OP_KIND, &[foo]);
    }
    let out = h.finish();
    let names: Vec<&str> = out.counters.iter().map(|c| c.name.as_str()).collect();
    assert!(names.windows(2).all(|n| n[0] < n[1]), "{names:?}");
    assert_eq!(out.counters.iter().find(|c| c.name == "api.op.lookup").map(|c| c.value), Some(3));
}

/// §7.6, §3.3: `canonical` is total: a group index out of range names no
/// edit and is ignored.
#[test]
fn s7_6_canonical_is_total() {
    let mut bad = raw();
    let n = bad.edits.len();
    bad.groups.push(vec![0, n, usize::MAX]);
    bad.groups.push(vec![n + 1, n + 2]);
    let mut good = raw();
    good.groups.push(vec![0]);
    assert_eq!(bad.canonical(), good.canonical());
}
