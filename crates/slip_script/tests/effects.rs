//! `spec/api.md` §7 (the effects: operations 64 to 84 of §3.2) and §4
//! rule 3 (a failed request has no effect), against the fake subject.
//! Each test names its section and rule.

#![allow(
    clippy::many_single_char_names,
    clippy::disallowed_names,
    reason = "the subject is the specification's example: lib::foo(a, b)"
)]

use slip_model::keys::{KeyKind, Name, Parent, Rule};
use std::sync::{Arc, Mutex};

use slip_model::{Digest, FileRef};
use slip_script::fake::{
    FakeCall, FakeDeclaration, FakeEntity, FakeExpression, FakeOut, FakeReference, FakeSubject,
};
use slip_script::host::{Host, Limits};
use slip_script::output::{Changed, Edit, Piece, Place, Report, RunOutput, Severity, Tiers};
use slip_script::subject::{
    Declaration, Entity, Expression, File, PlaceOf, Range, Reference, Subject, Value,
};
use slip_script::wire::{self, Errc};

// ---------------------------------------------------------------------------
// The subject.

const T_CPP: &str = "namespace lib { inline namespace inl { int foo(int a, int b); int bar(); } }\n\
namespace { int hidden; }\n\
namespace other { int foo(int a, int b); }\n\
int main() { return lib::foo(1, 2) + lib::foo(3, 4); }\n\
int k = M(lib::foo)(5, 6) + N::foo(7, 8) + P::foo(9, 0) + Q::foo + R::foo;\n";
const H_H: &str = "int h = lib::foo(0, 0);\n";

const K_FOO: Digest = Digest([0xf0; 32]);
const K_BAR: Digest = Digest([0xba; 32]);
const K_OTHER: Digest = Digest([0x07; 32]);
const K_HIDDEN: Digest = Digest([0x41; 32]);
const K_INL: Digest = Digest([0x11; 32]);
const U_MAIN: Digest = Digest([0x30; 32]);
const U_DERIVED1: Digest = Digest([0x31; 32]);
const U_DERIVED2: Digest = Digest([0x32; 32]);
const U_K: Digest = Digest([0x40; 32]);

#[allow(dead_code, reason = "a description; not every test uses every name")]
struct World {
    fake: FakeSubject,
    t: File,
    h: File,
    lib: Entity,
    inl: Entity,
    foo: Entity,
    a: Entity,
    bar: Entity,
    hidden: Entity,
    other_foo: Entity,
    /// A function the digest has no key for.
    main: Entity,
    /// An unnamed class, and a member of it.
    unnamed: Entity,
    inner: Entity,
    /// A member of a class template.
    tmember: Entity,
    foo_spec: Entity,
    /// `lib::foo` in the first call: direct, named, extent `lib::foo`.
    r1: Reference,
    /// `lib::foo` in the second call, to a specialisation of `foo`.
    r2: Reference,
    /// `N::foo`, found through a using-declaration.
    r_indirect: Reference,
    /// `P::foo`, with no name range.
    r_noname: Reference,
    /// Two uses of the argument of `M(lib::foo)`: one extent, tier 1.
    r_m1: Reference,
    r_m2: Reference,
    /// `Q::foo`, with no extent.
    r_noextent: Reference,
    /// `R::foo`, whose unit is keyed by content.
    r_content: Reference,
    /// `lib::foo` in `h.h`, a file the run may not edit.
    r_h: Reference,
    call1: Expression,
    arg1: Expression,
    /// An expression with no extent.
    x_null: Expression,
}

/// The name `foo` at the end of an extent.
fn name_in(e: Range) -> Range {
    Range { lo: e.hi - 3, ..e }
}

#[allow(clippy::too_many_lines, reason = "one literal description of the subject")]
fn world() -> World {
    let mut f = FakeSubject::new();
    let g = FakeSubject::GLOBAL;
    let t = f.file("/src/t.cpp", T_CPP, true);
    let h = f.file("/src/h.h", H_H, false);
    let lib = f.entity(FakeEntity::namespace("lib", g));
    let inl = f.entity(FakeEntity {
        transparent: true,
        key: Some(K_INL),
        ..FakeEntity::namespace("inl", lib)
    });
    let foo = f.entity(FakeEntity {
        key: Some(K_FOO),
        rule: Some((Parent::Unit(K_INL), KeyKind::Function, None)),
        ..FakeEntity::function("foo", inl, Vec::new())
    });
    let a =
        f.entity(FakeEntity { key: Some(Digest([0xaa; 32])), ..FakeEntity::parameter("a", foo) });
    f.entities[foo.0 as usize].parameters = Some(vec![a]);
    let bar = f.entity(FakeEntity {
        key: Some(K_BAR),
        rule: Some((Parent::Unit(K_INL), KeyKind::Function, None)),
        ..FakeEntity::function("bar", inl, Vec::new())
    });
    let anon = f.entity(FakeEntity { transparent: true, ..FakeEntity::namespace("", g) });
    let hidden = f.entity(FakeEntity {
        identifier: b"hidden".to_vec(),
        kinds: wire::KIND_VARIABLE,
        parent: Some(anon),
        key: Some(K_HIDDEN),
        ..FakeEntity::default()
    });
    let other = f.entity(FakeEntity::namespace("other", g));
    let other_foo = f.entity(FakeEntity {
        key: Some(K_OTHER),
        ..FakeEntity::function("foo", other, Vec::new())
    });
    let main = f.entity(FakeEntity::function("main", g, Vec::new()));
    let unnamed = f.entity(FakeEntity {
        transparent: true,
        key: Some(Digest([0x54; 32])),
        ..FakeEntity::class("", g)
    });
    let inner = f.entity(FakeEntity {
        key: Some(Digest([0x55; 32])),
        ..FakeEntity::function("inner", unnamed, Vec::new())
    });
    let tclass = f.entity(FakeEntity {
        kinds: wire::KIND_TYPE | wire::KIND_TEMPLATE,
        ..FakeEntity::class("C", g)
    });
    let tmember = f.entity(FakeEntity {
        key: Some(Digest([0x56; 32])),
        ..FakeEntity::function("m", tclass, Vec::new())
    });
    let foo_spec = f.entity(FakeEntity {
        origin: Some(foo),
        key: Some(Digest([0x57; 32])),
        ..FakeEntity::function("foo", inl, Vec::new())
    });

    let at = |f: &FakeSubject, file, needle: &str, nth| f.range_nth(file, needle, nth);
    let decl = |f: &mut FakeSubject, entity, name: Option<Range>| {
        f.declaration(FakeDeclaration { entity, name, definition: false });
    };
    for (e, needle) in
        [(lib, "lib"), (foo, "foo"), (bar, "bar"), (hidden, "hidden"), (main, "main")]
    {
        let r = at(&f, t, needle, 0);
        decl(&mut f, e, Some(r));
    }
    // Declarations with no name range: their places are the nearest.
    for e in [other, other_foo, unnamed, inner, tclass, tmember] {
        decl(&mut f, e, None);
    }

    let reference = |f: &mut FakeSubject, referent, extent: Option<Range>, name: Option<Range>| {
        f.reference(FakeReference { name, extent, ..FakeReference::to(referent, "foo") })
    };
    let e = at(&f, t, "lib::foo", 0);
    let r1 = reference(&mut f, foo, Some(e), Some(name_in(e)));
    f.references[r1.0 as usize].unit_keys = Ok(vec![U_MAIN, U_DERIVED1, U_DERIVED2]);
    let e = at(&f, t, "lib::foo", 1);
    let r2 = reference(&mut f, foo_spec, Some(e), Some(name_in(e)));
    f.references[r2.0 as usize].unit_keys = Ok(vec![U_MAIN]);
    let e = at(&f, t, "lib::foo", 2);
    f.tier1.push(e);
    let r_m1 = reference(&mut f, foo, Some(e), Some(name_in(e)));
    let r_m2 = reference(&mut f, foo, Some(e), Some(name_in(e)));
    let e = at(&f, t, "N::foo", 0);
    let r_indirect = reference(&mut f, foo, Some(e), Some(name_in(e)));
    f.references[r_indirect.0 as usize].direct = false;
    let e = at(&f, t, "P::foo", 0);
    let r_noname = reference(&mut f, foo, Some(e), None);
    let r_noextent = reference(&mut f, foo, None, None);
    f.references[r_noextent.0 as usize].unit_keys = Ok(vec![U_MAIN]);
    let e = at(&f, t, "R::foo", 0);
    let r_content = reference(&mut f, foo, Some(e), Some(name_in(e)));
    let e = at(&f, h, "lib::foo", 0);
    let r_h = reference(&mut f, foo, Some(e), Some(name_in(e)));

    let call1 = f.expression(FakeExpression {
        call: Some(FakeCall {
            callee: foo,
            reference: r1,
            arguments: vec![Expression(1), Expression(2)],
        }),
        unit_keys: Ok(vec![U_K]),
        ..FakeExpression::at(Some(at(&f, t, "lib::foo(1, 2)", 0)))
    });
    let arg1 = f.expression(FakeExpression::at(Some(at(&f, t, "1", 0))));
    let x_null =
        f.expression(FakeExpression { unit_keys: Ok(vec![U_K]), ..FakeExpression::at(None) });
    let near = at(&f, t, "other", 0);
    f.nearest.push((Value::Entity(other_foo), near));

    World {
        fake: f,
        t,
        h,
        lib,
        inl,
        foo,
        a,
        bar,
        hidden,
        other_foo,
        main,
        unnamed,
        inner,
        tmember,
        foo_spec,
        r1,
        r2,
        r_indirect,
        r_noname,
        r_m1,
        r_m2,
        r_noextent,
        r_content,
        r_h,
        call1,
        arg1,
        x_null,
    }
}

// ---------------------------------------------------------------------------
// Calling the host.

const ROOM: usize = 1 << 12;

fn call(h: &mut Host<'_>, op: u32, ints: &[u64], bytes: &[u8]) -> (u64, FakeOut) {
    let mut out = FakeOut::with_capacity(ROOM, ROOM);
    let word = h.call(op, ints, bytes, &mut out).expect("no host failure");
    (word, out)
}

fn errc(e: Errc) -> u8 {
    e.status()
}

/// One run over a subject, with the handles of the fake's values obtained
/// as a transform obtains them: by queries.
struct Run<'w, 's> {
    w: &'w World,
    h: Host<'s>,
}

impl<'w, 's> Run<'w, 's> {
    fn new(w: &'w World, s: &'s (dyn Subject + Sync)) -> Run<'w, 's> {
        Run::with_limits(w, s, Limits::default())
    }

    fn with_limits(w: &'w World, s: &'s (dyn Subject + Sync), limits: Limits) -> Run<'w, 's> {
        Run { w, h: Host::new(s, Digest([1; 32]), limits) }
    }

    /// The status of a call, which writes nothing.
    fn status(&mut self, op: u32, ints: &[u64], bytes: &[u8]) -> u8 {
        let (word, out) = call(&mut self.h, op, ints, bytes);
        assert_eq!(out.writes, 0, "an effect writes nothing");
        wire::status(word)
    }

    /// A call of convention V that must succeed: its value.
    fn ok(&mut self, op: u32, ints: &[u64], bytes: &[u8]) -> u64 {
        let (word, out) = call(&mut self.h, op, ints, bytes);
        assert_eq!(
            wire::status(word),
            0,
            "op {op} {ints:x?} {:?}: {:?}",
            String::from_utf8_lossy(bytes),
            self.h.last_failure()
        );
        assert_eq!(out.writes, 0);
        wire::payload(word)
    }

    /// A query of convention B that must succeed: its integers.
    fn ints(&mut self, op: u32, args: &[u64], bytes: &[u8]) -> Vec<u64> {
        let (word, out) = call(&mut self.h, op, args, bytes);
        assert_eq!(wire::status(word), 0, "op {op}: {:?}", self.h.last_failure());
        out.ints
    }

    /// The handle of a value of the fake.
    fn handle(&mut self, v: Value) -> u64 {
        let f = &self.w.fake;
        match v {
            Value::Entity(e) => {
                if f.entities[e.0 as usize].origin.is_some() {
                    // A specialisation: the referent of a reference to it.
                    let r = (0..f.references.len() as u32)
                        .map(Reference)
                        .find(|&r| f.referent(r) == e)
                        .expect("a reference to the specialisation");
                    let rh = self.handle(Value::Reference(r));
                    return self.ok(wire::OP_REFERENT_OF, &[rh], &[]);
                }
                if f.is_live(v) && f.entities[e.0 as usize].identifier.is_empty() && e.0 != 0 {
                    // An unnamed scope: the parent of a member.
                    let member = (0..f.entities.len() as u32)
                        .map(Entity)
                        .find(|&m| f.parent(m) == Some(e))
                        .expect("a member");
                    let mh = self.handle(Value::Entity(member));
                    return self.ok(wire::OP_PARENT_OF, &[mh], &[]);
                }
                if f.is(e, wire::KIND_PARAMETER) {
                    let fh = self.handle(Value::Entity(f.parent(e).expect("a function")));
                    let ps = self.ints(wire::OP_PARAMETERS_OF, &[fh], &[]);
                    let i = f
                        .parameters(Value::Entity(f.parent(e).expect("a function")))
                        .expect("parameters")
                        .iter()
                        .position(|&p| p == e)
                        .expect("a parameter of its function");
                    return ps[i];
                }
                let name = lookup_name(f, e);
                let found = self.ints(wire::OP_LOOKUP, &[], name.as_bytes());
                let parts: Vec<&[u8]> = name.split("::").map(str::as_bytes).collect();
                let i = f
                    .lookup(false, &parts)
                    .iter()
                    .position(|&c| c == e)
                    .unwrap_or_else(|| panic!("lookup of {name}"));
                found[i]
            }
            Value::Reference(r) => {
                let origin = f.origin(f.referent(r));
                let eh = self.handle(Value::Entity(origin));
                let all = self.ints(wire::OP_REFERENCES_TO, &[eh], &[]);
                let i = f.references_to(origin).iter().position(|&x| x == r).expect("listed");
                all[i]
            }
            Value::Expression(x) => {
                let w = self.w;
                let foo = self.handle(Value::Entity(w.foo));
                let call = self.ints(wire::OP_CALLS_TO, &[foo], &[])[0];
                if x == w.call1 {
                    return call;
                }
                let args = self.ints(wire::OP_ARGUMENTS_AS_WRITTEN, &[call], &[]);
                let i = f.arguments(w.call1).expect("a call").iter().position(|&a| a == x);
                args[i.expect("an argument of the call")]
            }
            Value::Declaration(d) => {
                let e = f.entity_of(d);
                let eh = self.handle(Value::Entity(e));
                let ds = self.ints(wire::OP_DECLARATIONS_OF, &[eh], &[]);
                let i = f.declarations(e).expect("declarations").iter().position(|&x| x == d);
                ds[i.expect("a declaration of its entity")]
            }
        }
    }

    fn entity(&mut self, e: Entity) -> u64 {
        self.handle(Value::Entity(e))
    }

    fn reference(&mut self, r: Reference) -> u64 {
        self.handle(Value::Reference(r))
    }

    fn expression(&mut self, x: Expression) -> u64 {
        self.handle(Value::Expression(x))
    }

    fn range_of(&mut self, op: u32, v: u64) -> [u64; 2] {
        let r = self.ints(op, &[v], &[]);
        assert_eq!(r.len(), 2, "a range");
        [r[0], r[1]]
    }

    /// The extent of a reference or an expression of the fake.
    fn extent(&mut self, v: Value) -> [u64; 2] {
        let h = self.handle(v);
        self.range_of(wire::OP_EXTENT_OF, h)
    }

    fn name_range(&mut self, v: Value) -> [u64; 2] {
        let h = self.handle(v);
        self.range_of(wire::OP_NAME_RANGE_OF, h)
    }

    fn text(&mut self, s: &str) -> u64 {
        self.ok(wire::OP_CODE_TEXT, &[], s.as_bytes())
    }

    fn spell(&mut self, e: Entity) -> u64 {
        let h = self.entity(e);
        self.ok(wire::OP_CODE_SPELL, &[h], &[])
    }

    fn copy(&mut self, r: [u64; 2]) -> u64 {
        self.ok(wire::OP_CODE_COPY, &r, &[])
    }

    fn cat(&mut self, codes: &[u64]) -> u64 {
        self.ok(wire::OP_CODE_CAT, codes, &[])
    }

    fn group(&mut self) -> u64 {
        self.ok(wire::OP_NEW_GROUP, &[], &[])
    }

    fn replace(&mut self, r: [u64; 2], code: u64, group: u64, note: &str) {
        self.ok(wire::OP_REPLACE, &[r[0], r[1], code, group], note.as_bytes());
    }

    fn try_replace(&mut self, r: [u64; 2], code: u64, group: u64, note: &str) -> u8 {
        self.status(wire::OP_REPLACE, &[r[0], r[1], code, group], note.as_bytes())
    }

    fn finish(self) -> RunOutput {
        self.h.finish()
    }
}

/// The name `lookup` finds `e` by: its named enclosing scopes that are not
/// transparent, and its identifier.
fn lookup_name(f: &FakeSubject, e: Entity) -> String {
    let mut parts = Vec::new();
    let mut x = e;
    while let Some(p) = f.parent(x) {
        let fe = &f.entities[x.0 as usize];
        if !fe.transparent || x == e {
            parts.push(String::from_utf8(fe.identifier.clone()).expect("ASCII"));
        }
        x = p;
    }
    parts.reverse();
    parts.join("::")
}

trait Is {
    fn is(&self, e: Entity, kind: u64) -> bool;
}

impl Is for FakeSubject {
    fn is(&self, e: Entity, kind: u64) -> bool {
        self.entities[e.0 as usize].kinds & kind != 0
    }
}

fn place(f: &FakeSubject, r: Range) -> Place {
    Place { file: Arc::new(f.file_ref(r.file)), lo: r.lo, hi: r.hi }
}

fn lit(s: &str) -> Piece {
    Piece::Literal(s.as_bytes().to_vec())
}

fn spelled(key: Digest, text: &str) -> Piece {
    Piece::Spelled { key, text: text.as_bytes().to_vec() }
}

/// The edits of an output as (lo, hi, pieces).
fn edits(out: &RunOutput) -> Vec<(u32, u32, Vec<Piece>)> {
    out.edits.iter().map(|e| (e.place.lo, e.place.hi, e.pieces.clone())).collect()
}

fn range(w: &World, needle: &str, nth: usize) -> Range {
    w.fake.range_nth(w.t, needle, nth)
}

fn span(r: Range) -> (u32, u32) {
    (r.lo, r.hi)
}

fn counter(out: &RunOutput, name: &str) -> Option<u64> {
    out.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

// ---------------------------------------------------------------------------
// §7.1 Code values.

/// §7.1: each kind of piece is recorded as made.
#[test]
fn s7_1_each_piece_kind() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let call_r = run.extent(Value::Expression(w.call1));
    let arg_r = run.extent(Value::Expression(w.arg1));
    let t = run.text("g(");
    let c = run.copy(arg_r);
    let s = run.spell(w.bar);
    let close = run.text(")");
    let code = run.cat(&[t, s, c, close]);
    run.replace(call_r, code, 0, "");
    let out = run.finish();
    let (lo, hi) = span(range(&w, "lib::foo(1, 2)", 0));
    assert_eq!(
        edits(&out),
        vec![(
            lo,
            hi,
            vec![
                lit("g("),
                spelled(K_BAR, "::lib::inl::bar"),
                Piece::Copy(place(&w.fake, range(&w, "1", 0))),
                lit(")")
            ]
        )]
    );
    assert_eq!(out.edits[0].tiers, Tiers::ZERO);
    assert_eq!(*out.edits[0].place.file, w.fake.file_ref(w.t));
    assert!(out.is_canonical());
}

/// §7.1 rule 5: `cat` takes every code (a text, a copy, a spelled entity,
/// another `cat`, the empty code); the header turns a range and a string
/// into a code before the call. The pieces are its arguments' in order.
#[test]
fn s7_1_cat_of_each_accepted_argument_sort() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let e = run.extent(Value::Reference(w.r1));
    let x = run.text("x");
    let c = run.copy(e);
    let s = run.spell(w.bar);
    let inner = run.cat(&[c, x]);
    let code = run.cat(&[x, 0, inner, s, 0]);
    assert_eq!(run.cat(&[]), 0, "the cat of nothing is the empty code");
    assert_eq!(run.cat(&[0, 0]), 0);
    let target = run.extent(Value::Expression(w.call1));
    run.replace(target, code, 0, "");
    let out = run.finish();
    let lib_foo = place(&w.fake, range(&w, "lib::foo", 0));
    assert_eq!(
        out.edits[0].pieces,
        vec![lit("x"), Piece::Copy(lib_foo), lit("x"), spelled(K_BAR, "::lib::inl::bar")]
    );
    // A handle that is not a code: invalid_handle.
    let mut run = Run::new(&w, &w.fake);
    let foo = run.entity(w.foo);
    let g = run.group();
    let x = run.text("x");
    for bad in [foo, g, x + 1, wire::handle(wire::TAG_CODE, 1 << 40)] {
        assert_eq!(run.status(wire::OP_CODE_CAT, &[x, bad], &[]), errc(Errc::InvalidHandle));
    }
}

/// §7.1 rule 6: adjacent literals are one piece, an empty literal is no
/// piece, and code values with the same pieces are interchangeable.
#[test]
fn s7_1_rule_6_literals_and_identity() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    assert_eq!(run.text(""), 0, "an empty literal is no piece: the empty code");
    let a = run.text("a");
    let b = run.text("b");
    let empty = run.text("");
    let ab = run.cat(&[a, empty, b]);
    let ab2 = run.text("ab");
    let ab3 = run.cat(&[ab2]);
    let target = run.extent(Value::Expression(w.call1));
    for code in [ab, ab2, ab3] {
        run.replace(target, code, 0, "");
    }
    let out = run.finish();
    assert_eq!(out.edits.len(), 1, "three requests with the same pieces are one edit");
    assert_eq!(out.edits[0].pieces, vec![lit("ab")]);
    assert_eq!(counter(&out, "api.op.replace"), Some(3));
    assert_eq!(counter(&out, "api.edits"), Some(1));
}

/// §7.1 rule 3 (a) and (b): two different edits of one range, and
/// `replace(x, copy(r))` with `x` properly inside `r`, are recorded as
/// requested, with no throw: the host does not judge them.
#[test]
fn s7_1_rule_3_conflicts_are_recorded_not_judged() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let call_r = run.extent(Value::Expression(w.call1));
    let arg_r = run.extent(Value::Expression(w.arg1));
    let x = run.text("x");
    let y = run.text("y");
    run.replace(call_r, x, 0, "");
    run.replace(call_r, y, 0, "");
    let around = run.copy(call_r);
    run.replace(arg_r, around, 0, "");
    let out = run.finish();
    let c = range(&w, "lib::foo(1, 2)", 0);
    let (alo, ahi) = span(range(&w, "1", 0));
    assert_eq!(
        edits(&out),
        vec![
            (c.lo, c.hi, vec![lit("x")]),
            (c.lo, c.hi, vec![lit("y")]),
            (alo, ahi, vec![Piece::Copy(place(&w.fake, c))]),
        ]
    );
}

/// §7.1 rule 2: `copy` of the null range is `invalid_handle`; so is a
/// range in a file never given out, or not inside its file.
#[test]
fn s7_1_rule_2_copy_refusals() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let [file, span] = run.extent(Value::Reference(w.r1));
    let ih = errc(Errc::InvalidHandle);
    assert_eq!(run.status(wire::OP_CODE_COPY, &[0, 0], &[]), ih);
    assert_eq!(run.status(wire::OP_CODE_COPY, &[file + 1, span], &[]), ih);
    assert_eq!(run.status(wire::OP_CODE_COPY, &[file, 1 << 32 | 1 << 20], &[]), ih);
    assert_eq!(run.status(wire::OP_CODE_COPY, &[file, 5 << 32 | 4], &[]), ih, "lo > hi");
    // An empty range is a range: its copy is not refused (and is the empty
    // code, `s7_1_rule_2_a_copy_of_an_empty_range_is_the_empty_code`).
    assert_eq!(run.ok(wire::OP_CODE_COPY, &[file, 4 << 32 | 4], &[]), 0);
}

/// §7.1 rule 2: the copy of an empty range is the empty code, the value
/// `code()` gives: the bytes of an empty range are none. `cat` with it adds
/// no piece, `replace` with it is a deletion (§7.3 rule 1), and the output
/// holds no copy piece with `lo == hi` (`spec/edits.md` §4.2). It counts
/// as one operation and holds nothing.
#[test]
fn s7_1_rule_2_a_copy_of_an_empty_range_is_the_empty_code() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let [file, sp] = run.extent(Value::Reference(w.r1));
    let (lo, hi) = (sp >> 32, sp & 0xffff_ffff);
    let empty_at = |at: u64| [file, at << 32 | at];
    let starts = run.copy(empty_at(lo));
    let ends = run.copy(empty_at(hi));
    assert_eq!((starts, ends), (0, 0), "the empty code: the null handle");
    // `cat` with it: no piece.
    let x = run.text("x");
    let cat = run.cat(&[starts, x, ends]);
    let target = run.extent(Value::Expression(w.call1));
    run.replace(target, cat, 0, "");
    // `replace` with it: a deletion.
    let arg = run.extent(Value::Expression(w.arg1));
    run.replace(arg, starts, 0, "");
    let out = run.finish();
    let c = range(&w, "lib::foo(1, 2)", 0);
    let a = range(&w, "1", 0);
    assert_eq!(edits(&out), vec![(c.lo, c.hi, vec![lit("x")]), (a.lo, a.hi, vec![])]);
    assert!(no_empty_copy(&out));
    assert_eq!(counter(&out, "api.op.code_copy"), Some(2), "each counts as one operation");
}

/// `spec/edits.md` §4.2: a copy piece's place is not empty.
fn no_empty_copy(out: &RunOutput) -> bool {
    out.edits
        .iter()
        .flat_map(|e| &e.pieces)
        .all(|p| !matches!(p, Piece::Copy(at) if at.lo == at.hi))
}

/// §7.1 rule 4: `spell` of a non-entity is `wrong_kind`; of a
/// specialisation, a member of a class template, a parameter, an entity
/// with no identifier, an entity declared in an unnamed class, and an
/// entity the digest has no key for, `unsupported`.
#[test]
fn s7_1_rule_4_spell_refusals() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let r = run.reference(w.r1);
    let spell = |run: &mut Run<'_, '_>, x: u64| run.status(wire::OP_CODE_SPELL, &[x], &[]);
    assert_eq!(spell(&mut run, r), errc(Errc::WrongKind), "a reference");
    assert_eq!(spell(&mut run, 0), errc(Errc::InvalidHandle));
    for (e, what) in [
        (w.foo_spec, "a specialisation"),
        (w.tmember, "a member of a class template"),
        (w.a, "a parameter"),
        (w.unnamed, "no identifier"),
        (w.inner, "declared in an unnamed class"),
        (w.main, "no key"),
    ] {
        let h = run.entity(e);
        assert_eq!(spell(&mut run, h), errc(Errc::Unsupported), "{what}");
    }
    let out = run.finish();
    assert_eq!(counter(&out, "api.op.code_spell"), None, "no spell succeeded");
}

// ---------------------------------------------------------------------------
// §7.2 The spelling of an entity.

/// One run: `replace(extent_of(R), spell(E))`; the edits recorded.
fn sibling_run(w: &World, r: Reference, e: Entity) -> Vec<(u32, u32, Vec<Piece>)> {
    let mut run = Run::new(w, &w.fake);
    let extent = run.extent(Value::Reference(r));
    let code = run.spell(e);
    run.replace(extent, code, 0, "");
    edits(&run.finish())
}

/// §7.2 rule 1: when the four conditions hold the edit replaces the name
/// range with the identifier; the qualifier stays as written.
#[test]
fn s7_2_rule_1_all_four_conditions_narrow_the_edit() {
    let w = world();
    let (lo, hi) = span(name_in(range(&w, "lib::foo", 0)));
    assert_eq!(sibling_run(&w, w.r1, w.bar), vec![(lo, hi, vec![spelled(K_BAR, "bar")])]);
    // The referent of r2 is a specialisation: its origin's parent counts.
    let (lo, hi) = span(name_in(range(&w, "lib::foo", 1)));
    assert_eq!(sibling_run(&w, w.r2, w.bar), vec![(lo, hi, vec![spelled(K_BAR, "bar")])]);
}

/// §7.2 rules 1 and 2: each condition failed alone gives the fully
/// qualified text, at the extent. Condition 1 ("E has an identifier")
/// cannot fail alone: `spell` refuses such an entity (§7.1 rule 4,
/// `s7_1_rule_4_spell_refusals`).
#[test]
fn s7_2_each_condition_failed_alone_is_fully_qualified() {
    let w = world();
    // Condition 2: E's parent is not the parent of the referent's origin.
    let (lo, hi) = span(range(&w, "lib::foo", 0));
    assert_eq!(
        sibling_run(&w, w.r1, w.other_foo),
        vec![(lo, hi, vec![spelled(K_OTHER, "::other::foo")])]
    );
    // Condition 3: found through a using-declaration.
    let (lo, hi) = span(range(&w, "N::foo", 0));
    assert_eq!(
        sibling_run(&w, w.r_indirect, w.bar),
        vec![(lo, hi, vec![spelled(K_BAR, "::lib::inl::bar")])]
    );
    // Condition 4: no name range.
    let (lo, hi) = span(range(&w, "P::foo", 0));
    assert_eq!(
        sibling_run(&w, w.r_noname, w.bar),
        vec![(lo, hi, vec![spelled(K_BAR, "::lib::inl::bar")])]
    );
}

/// §7.2 rule 1: the rule is for a code that is exactly one spelled piece:
/// two pieces at an extent are fully qualified; an empty literal beside
/// the piece is no piece. At a name range the sibling form applies too
/// (WP-132; it was fully qualified before), so the spelling at the name
/// range and the one narrowed from the extent are one edit.
#[test]
fn s7_2_rule_1_only_one_spelled_piece() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let extent = run.extent(Value::Reference(w.r1));
    let name = run.name_range(Value::Reference(w.r1));
    let s = run.spell(w.bar);
    let x = run.text("x");
    let two = run.cat(&[s, x]);
    let empty = run.text("");
    let one = run.cat(&[empty, s, 0]);
    run.replace(extent, two, 0, "");
    run.replace(name, s, 0, "");
    run.replace(extent, one, 0, "");
    let out = run.finish();
    let (lo, hi) = span(range(&w, "lib::foo", 0));
    assert_eq!(
        edits(&out),
        vec![
            (lo, hi, vec![spelled(K_BAR, "::lib::inl::bar"), lit("x")]),
            (hi - 3, hi, vec![spelled(K_BAR, "bar")]),
        ]
    );
}

/// §7.2 rule 1, last sentence: two references with one extent (two uses
/// of a macro argument, tier 1): the rule holds for both, and the edit is
/// recorded once, at tiers {1}; if it fails for one, it is fully qualified.
#[test]
fn s7_2_two_references_with_one_extent() {
    let w = world();
    let m = range(&w, "lib::foo", 2);
    let (lo, hi) = span(name_in(m));
    let run_with = |f: &FakeSubject| {
        let mut run = Run::new(&w, f);
        let e = run.extent(Value::Reference(w.r_m1));
        let e2 = run.extent(Value::Reference(w.r_m2));
        assert_eq!(e, e2, "one extent");
        let s = run.spell(w.bar);
        run.replace(e, s, 0, "a");
        run.replace(e2, s, 0, "b");
        run.finish()
    };
    let out = run_with(&w.fake);
    assert_eq!(edits(&out), vec![(lo, hi, vec![spelled(K_BAR, "bar")])]);
    assert_eq!(out.edits[0].tiers, Tiers::ONE, "every request at tier 1: tiers 1");
    assert_eq!(out.edits[0].notes, vec![b"a".to_vec(), b"b".to_vec()]);
    // One of the two found indirectly: no narrowing for either.
    let mut f = w.fake.clone();
    f.references[w.r_m2.0 as usize].direct = false;
    let out = run_with(&f);
    assert_eq!(edits(&out), vec![(m.lo, m.hi, vec![spelled(K_BAR, "::lib::inl::bar")])]);
    // Two name ranges for one extent: no narrowing (§7.2 rule 1).
    let mut f = w.fake.clone();
    f.references[w.r_m2.0 as usize].name = Some(Range { lo: m.lo, hi: m.lo + 3, ..m });
    let out = run_with(&f);
    assert_eq!(edits(&out), vec![(m.lo, m.hi, vec![spelled(K_BAR, "::lib::inl::bar")])]);
}

/// §7.2 rule 1 at a name range (WP-132): the sibling's identifier at the
/// name range of a reference and at a declaration's name; any other
/// entity, or another piece beside the spelled one, is `invalid_argument`
/// and records nothing.
#[test]
fn s7_2_rule_1_at_a_name_range() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let name = run.name_range(Value::Reference(w.r1));
    let foo_decl = w.fake.declarations(w.foo).expect("declarations")[0];
    let decl_name = run.name_range(Value::Declaration(foo_decl));
    let bar = run.spell(w.bar);
    let other = run.spell(w.other_foo);
    let x = run.text("x");
    let with_text = run.cat(&[bar, x]);
    let id = run.ok(wire::OP_CODE_IDENTIFIER, &[], b"baz");
    let with_id = run.cat(&[bar, id]);
    for (r, code) in [(name, other), (name, with_text), (name, with_id), (decl_name, other)] {
        assert_eq!(run.try_replace(r, code, 0, ""), errc(Errc::InvalidArgument));
    }
    run.replace(name, bar, 0, "");
    run.replace(decl_name, bar, 0, "");
    run.replace(decl_name, id, 0, "");
    let out = run.finish();
    let (lo, hi) = span(name_in(range(&w, "lib::foo", 0)));
    let d = w.fake.range_nth(w.t, "foo", 0);
    assert_eq!(
        edits(&out),
        vec![
            (d.lo, d.hi, vec![spelled(K_BAR, "bar")]),
            (d.lo, d.hi, vec![Piece::Identifier(b"baz".to_vec())]),
            (lo, hi, vec![spelled(K_BAR, "bar")]),
        ]
    );
}

/// §7.2 rule 1 at a name range: a reference named there found indirectly,
/// or whose origin is no sibling, refuses the spelling; the range of two
/// uses of a macro argument is one name range for both.
#[test]
fn s7_2_rule_1_at_a_name_range_every_name_counts() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let indirect = run.name_range(Value::Reference(w.r_indirect));
    let bar = run.spell(w.bar);
    assert_eq!(run.try_replace(indirect, bar, 0, ""), errc(Errc::InvalidArgument));
    let m = run.name_range(Value::Reference(w.r_m1));
    run.replace(m, bar, 0, "");
    let out = run.finish();
    let (lo, hi) = span(name_in(range(&w, "lib::foo", 2)));
    assert_eq!(edits(&out), vec![(lo, hi, vec![spelled(K_BAR, "bar")])]);
    assert_eq!(out.edits[0].tiers, Tiers::ONE);
}

/// §7.1 rule 7 (WP-132): `identifier(name)` makes one identifier piece,
/// kept apart from the literals and identifiers beside it, and counted;
/// the name is one identifier of §6.1 rule 1.
#[test]
fn s7_1_rule_7_identifier() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let target = run.extent(Value::Expression(w.arg1));
    let a = run.ok(wire::OP_CODE_IDENTIFIER, &[], b"a_1");
    let b = run.ok(wire::OP_CODE_IDENTIFIER, &[], b"B");
    let x = run.text("+");
    let code = run.cat(&[a, x, b, a]);
    run.replace(target, code, 0, "");
    let refused: [(&[u8], Errc); 9] = [
        (b"", Errc::InvalidArgument),
        (b"class", Errc::InvalidArgument),
        (b"\xff", Errc::InvalidArgument),
        (b"1a", Errc::InvalidArgument),
        (b"a b", Errc::InvalidArgument),
        (b"a::b", Errc::InvalidArgument),
        (b"a+", Errc::InvalidArgument),
        (b"a$", Errc::Unsupported),
        ("\u{e9}".as_bytes(), Errc::Unsupported),
    ];
    for (bad, e) in refused {
        assert_eq!(run.status(wire::OP_CODE_IDENTIFIER, &[], bad), errc(e), "{bad:?}");
    }
    let long = vec![b'a'; 4097];
    assert_eq!(run.status(wire::OP_CODE_IDENTIFIER, &[], &long), errc(Errc::LimitExceeded));
    let out = run.finish();
    let id = |s: &str| Piece::Identifier(s.as_bytes().to_vec());
    assert_eq!(out.edits[0].pieces, vec![id("a_1"), lit("+"), id("B"), id("a_1")]);
    assert_eq!(counter(&out, "api.op.code_identifier"), Some(2));
}

/// §6.1 rule 8 (WP-132): `lookup(scope, identifier)` on the fake: an
/// entity that is no scope is `wrong_kind`; text that is not one
/// identifier is `invalid_argument`; the members are found through
/// transparent scopes.
#[test]
fn s6_1_rule_8_lookup_in() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let lib = run.entity(w.lib);
    let foo = run.entity(w.foo);
    assert_eq!(run.ints(wire::OP_LOOKUP_IN, &[lib], b"foo").len(), 1, "through inl");
    assert!(run.ints(wire::OP_LOOKUP_IN, &[lib], b"nope").is_empty());
    assert_eq!(run.status(wire::OP_LOOKUP_IN, &[foo], b"a"), errc(Errc::WrongKind));
    assert_eq!(run.status(wire::OP_LOOKUP_IN, &[lib], b"inl::foo"), errc(Errc::InvalidArgument));
    assert_eq!(run.status(wire::OP_LOOKUP_IN, &[lib], b""), errc(Errc::InvalidArgument));
    assert_eq!(run.status(wire::OP_LOOKUP_IN, &[1], b"foo"), errc(Errc::InvalidHandle));
}

/// §7.2 rule 2: unnamed namespaces are skipped, inline ones named.
#[test]
fn s7_2_rule_2_unnamed_and_inline_namespaces() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let target = run.extent(Value::Expression(w.call1));
    let hidden = run.spell(w.hidden);
    let foo = run.spell(w.foo);
    let code = run.cat(&[hidden, foo]);
    run.replace(target, code, 0, "");
    let out = run.finish();
    assert_eq!(
        out.edits[0].pieces,
        vec![spelled(K_HIDDEN, "::hidden"), spelled(K_FOO, "::lib::inl::foo")]
    );
}

// ---------------------------------------------------------------------------
// §7.3 replace and groups.

/// §7.3 rule 2 and the arguments: each throw of `replace`.
#[test]
fn s7_3_each_throw() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let [file, sp] = run.extent(Value::Expression(w.call1));
    let in_h = run.extent(Value::Reference(w.r_h));
    let x = run.text("x");
    let g = run.group();
    let foo = run.entity(w.foo);
    let ih = errc(Errc::InvalidHandle);
    let mut replace = |ints: [u64; 4]| run.status(wire::OP_REPLACE, &ints, b"");
    assert_eq!(replace([0, 0, x, 0]), ih, "the null range");
    assert_eq!(replace([file + 5, sp, x, 0]), ih, "a file never given out");
    assert_eq!(replace([file, 1 << 32 | 1 << 20, x, 0]), ih, "past the end");
    assert_eq!(replace([file, sp, x + 1, 0]), ih, "a code never made");
    assert_eq!(replace([file, sp, foo, 0]), ih, "an entity for a code");
    assert_eq!(replace([file, sp, x, g + 1]), ih, "a group never made");
    assert_eq!(replace([file, sp, x, x]), ih, "a code for a group");
    let lo = sp >> 32;
    assert_eq!(replace([file, lo << 32 | lo, x, 0]), errc(Errc::InvalidArgument), "empty");
    assert_eq!(replace([in_h[0], in_h[1], x, 0]), errc(Errc::NotEditable));
    // A range of the unit that no query returns: forged (§7.3 rule 2).
    assert_eq!(replace([file, 1 << 32 | 2, x, 0]), ih, "a range no query returns");
    let out = run.finish();
    assert!(out.edits.is_empty());
}

/// §7.3 rule 2, §5.2 rule 2: `replace` takes only a range that
/// `name_range_of` or `extent_of` returns. A range built inside one (part
/// of a macro invocation, half a token) is forged and refused, and records
/// nothing; a forged range equal to a real one is that one, with its tiers.
#[test]
fn s7_3_rule_2_a_range_no_query_returns_is_forged() {
    let mut f = FakeSubject::new();
    let t = f.file("/src/m.cpp", "int k = M(foo) + foo;\n", true);
    let foo = f.entity(FakeEntity {
        identifier: b"foo".to_vec(),
        kinds: wire::KIND_VARIABLE,
        parent: Some(FakeSubject::GLOBAL),
        ..FakeEntity::default()
    });
    let arg = f.range_nth(t, "foo", 0);
    let plain = f.range_nth(t, "foo", 1);
    let whole = f.range_nth(t, "M(foo) + foo", 0);
    assert_eq!((span(arg), span(plain), span(whole)), ((10, 13), (17, 20), (8, 20)));
    f.tier1.push(arg);
    for r in [arg, plain] {
        f.reference(FakeReference {
            name: Some(r),
            extent: Some(r),
            ..FakeReference::to(foo, "foo")
        });
    }
    f.expression(FakeExpression::at(Some(whole)));
    f.declaration(FakeDeclaration { entity: foo, name: None, definition: false });
    let mut h = Host::new(&f, Digest([1; 32]), Limits::default());
    let ints = |h: &mut Host<'_>, op, args: &[u64], bytes: &[u8]| {
        let (word, out) = call(h, op, args, bytes);
        assert_eq!(wire::status(word), 0, "op {op}");
        (wire::payload(word), out.ints)
    };
    let foo_h = ints(&mut h, wire::OP_LOOKUP, &[], b"foo").1[0];
    let refs = ints(&mut h, wire::OP_REFERENCES_TO, &[foo_h], &[]).1;
    let name = ints(&mut h, wire::OP_NAME_RANGE_OF, &[refs[0]], &[]).1;
    let file = name[0];
    let x = ints(&mut h, wire::OP_CODE_TEXT, &[], b"x").0;
    let replace = |h: &mut Host<'_>, lo: u64, hi: u64| {
        wire::status(call(h, wire::OP_REPLACE, &[file, lo << 32 | hi, x, 0], b"").0)
    };
    let ih = errc(Errc::InvalidHandle);
    assert_eq!(replace(&mut h, 8, 11), ih, "`M(f`: inside the extent, not one");
    assert_eq!(replace(&mut h, 18, 19), ih, "half a token");
    assert_eq!(replace(&mut h, 8, 13), ih, "`M(foo`");
    assert_eq!(replace(&mut h, 0, 22), ih, "the whole file");
    assert_eq!(replace(&mut h, 8, 20), 0, "the extent, never asked for: it is that range");
    assert_eq!(name[1], (10 << 32) + 13);
    assert_eq!(replace(&mut h, 10, 13), 0, "the name range");
    assert_eq!(replace(&mut h, 17, 20), 0, "the other name range");
    let out = h.finish();
    let got: Vec<_> = out.edits.iter().map(|e| (e.place.lo, e.place.hi, e.tiers)).collect();
    assert_eq!(got, vec![(8, 20, Tiers::ZERO), (10, 13, Tiers::ONE), (17, 20, Tiers::ZERO)]);
    assert_eq!(counter(&out, "api.op.replace"), Some(3));
}

/// §7.3 rule 1: the empty code requests deletion; the default group and
/// note are none.
#[test]
fn s7_3_rule_1_a_deletion() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let target = run.extent(Value::Expression(w.arg1));
    run.replace(target, 0, 0, "");
    let empty = run.text("");
    run.replace(target, empty, 0, "");
    let out = run.finish();
    let (lo, hi) = span(range(&w, "1", 0));
    assert_eq!(edits(&out), vec![(lo, hi, vec![])]);
    assert!(out.edits[0].notes.is_empty());
    assert!(out.groups.is_empty());
}

/// §7.3 rules 3 and 5, §7.6 rules 1 and 2: overlapping requests are both
/// recorded; the same request twice is one edit with both notes; a group
/// of one is dropped; two groups sharing an edit are one.
#[test]
fn s7_3_overlap_identity_and_groups() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let call = run.extent(Value::Expression(w.call1));
    let arg = run.extent(Value::Expression(w.arg1));
    let name = run.name_range(Value::Reference(w.r1));
    let other = run.extent(Value::Reference(w.r_indirect));
    let x = run.text("x");
    let y = run.text("y");
    let g1 = run.group();
    let g2 = run.group();
    let g3 = run.group();
    let lonely = run.group();
    run.replace(call, x, g1, "first");
    run.replace(arg, y, g1, "");
    run.replace(call, x, g2, "second"); // the same edit: g1 and g2 are one
    run.replace(other, y, g3, "");
    run.replace(name, x, 0, "");
    run.replace(arg, x, lonely, ""); // a group of one
    let _ = g3;
    let out = run.finish();
    let c = range(&w, "lib::foo(1, 2)", 0);
    let a = range(&w, "1", 0);
    let n = name_in(range(&w, "lib::foo", 0));
    let o = range(&w, "N::foo", 0);
    assert_eq!(
        edits(&out),
        vec![
            (c.lo, c.hi, vec![lit("x")]),
            (n.lo, n.hi, vec![lit("x")]),
            (a.lo, a.hi, vec![lit("x")]),
            (a.lo, a.hi, vec![lit("y")]),
            (o.lo, o.hi, vec![lit("y")]),
        ]
    );
    assert_eq!(out.edits[0].notes, vec![b"first".to_vec(), b"second".to_vec()]);
    assert_eq!(out.groups, vec![vec![0, 3]], "g1 and g2 merged; g3 and lonely of one dropped");
    // §7.7: the run holds the file's identity once, whatever names it.
    let first = &out.edits[0].place.file;
    assert!(out.edits.iter().all(|e| Arc::ptr_eq(&e.place.file, first)));
}

// ---------------------------------------------------------------------------
// §7.4 Reports.

fn report(run: &mut Run<'_, '_>, ints: [u64; 4], message: &str) -> u8 {
    run.status(wire::OP_REPORT, &ints, message.as_bytes())
}

/// §7.4: each overload; an unknown severity; a place that is not the
/// value's own; a value whose place the host cannot name (no place, and
/// says so: `attached`); an entity with no written declaration (no place,
/// nothing to attach to); a where-kind the host does not have.
#[test]
fn s7_4_reports() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let r = run.reference(w.r1);
    let call = run.expression(w.call1);
    let foo = run.entity(w.foo);
    let other = run.entity(w.other_foo);
    let decl = run.handle(Value::Declaration(Declaration(0)));
    let tclass_member = run.entity(w.tmember);
    // `inl` has no written declaration: reached as `foo`'s parent.
    let inl = run.ok(wire::OP_PARENT_OF, &[foo], &[]);
    let [file, sp] = run.extent(Value::Expression(w.arg1));
    assert_eq!(report(&mut run, [0, 0, 0, 0], "unit"), 0);
    assert_eq!(report(&mut run, [1, r, 0, 1], "ref"), 0);
    assert_eq!(report(&mut run, [1, call, 0, 2], "call"), 0);
    assert_eq!(report(&mut run, [1, foo, 0, 0], "entity"), 0);
    assert_eq!(report(&mut run, [1, decl, 0, 0], "declaration"), 0);
    assert_eq!(report(&mut run, [1, other, 0, 1], "nearest"), 0);
    assert_eq!(report(&mut run, [1, tclass_member, 0, 0], "unnamed"), 0);
    assert_eq!(report(&mut run, [1, inl, 0, 0], "nowhere"), 0);
    assert_eq!(report(&mut run, [2, file, sp, 1], "range"), 0);
    let ia = errc(Errc::InvalidArgument);
    assert_eq!(report(&mut run, [0, 0, 0, 3], "x"), ia, "an unknown severity");
    assert_eq!(report(&mut run, [0, 0, 0, u64::MAX], "x"), ia);
    assert_eq!(report(&mut run, [3, 0, 0, 0], "x"), ia, "an unknown where-kind");
    assert_eq!(report(&mut run, [0, 1, 0, 0], "x"), ia, "where-kind 0 with a value");
    assert_eq!(report(&mut run, [1, r, 1, 0], "x"), ia, "where-kind 1 with a second integer");
    assert_eq!(report(&mut run, [1, 0, 0, 0], "x"), errc(Errc::InvalidHandle));
    assert_eq!(report(&mut run, [2, 0, 0, 0], "x"), errc(Errc::InvalidHandle));
    let out = run.finish();
    let f = &w.fake;
    let at = |needle: &str, nth| Some(place(f, range(&w, needle, nth)));
    let rep = |p: Option<Place>, s, m: &str, attached| Report {
        place: p,
        severity: s,
        message: m.as_bytes().to_vec(),
        attached,
    };
    let mut expected = vec![
        rep(None, Severity::Note, "unit", false),
        rep(at("lib::foo", 0), Severity::Review, "ref", false),
        rep(at("lib::foo(1, 2)", 0), Severity::Error, "call", false),
        rep(at("foo", 0), Severity::Note, "entity", false),
        rep(at("lib", 0), Severity::Note, "declaration", false),
        rep(at("other", 0), Severity::Review, "nearest", true),
        rep(None, Severity::Note, "unnamed", true),
        rep(None, Severity::Note, "nowhere", false),
        rep(at("1", 0), Severity::Review, "range", false),
    ];
    expected.sort();
    assert_eq!(out.reports, expected);
    assert_eq!(counter(&out, "api.reports"), Some(9));
}

// ---------------------------------------------------------------------------
// §7.5 Declared changes.

fn changed(run: &mut Run<'_, '_>, v: u64, why: &str) -> u8 {
    run.status(wire::OP_EXPECT_CHANGED, &[v], why.as_bytes())
}

fn renamed(run: &mut Run<'_, '_>, e: u64, name: &str) -> u8 {
    run.status(wire::OP_EXPECT_RENAMED, &[e], name.as_bytes())
}

/// §7.5 rules 1, 2, 4 and 7: `expect_changed` at a reference and at an
/// expression records the unit's key and the derived keys, each with the
/// extent as the place; each refusal; the same declaration twice is one.
#[test]
fn s7_5_expect_changed() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let r = run.reference(w.r1);
    let r2 = run.reference(w.r2);
    let call = run.expression(w.call1);
    let foo = run.entity(w.foo);
    let decl = run.handle(Value::Declaration(Declaration(1)));
    let noextent = run.reference(w.r_noextent);
    let x_null = run.expression(w.x_null);
    let content = run.reference(w.r_content);
    assert_eq!(changed(&mut run, r, "renamed"), 0);
    assert_eq!(changed(&mut run, r, "renamed"), 0, "twice is once");
    assert_eq!(changed(&mut run, r2, "renamed"), 0, "the same unit at another place");
    assert_eq!(changed(&mut run, call, "wrapped"), 0);
    let unsupported = errc(Errc::Unsupported);
    assert_eq!(changed(&mut run, foo, "x"), unsupported, "an entity");
    assert_eq!(changed(&mut run, decl, "x"), unsupported, "a declaration");
    assert_eq!(changed(&mut run, noextent, "x"), unsupported, "a reference with no extent");
    assert_eq!(changed(&mut run, x_null, "x"), unsupported, "an expression with no extent");
    assert_eq!(changed(&mut run, content, "x"), unsupported, "a unit keyed by content");
    assert_eq!(changed(&mut run, 0, "x"), errc(Errc::InvalidHandle), "a null info");
    let out = run.finish();
    let p1 = place(&w.fake, range(&w, "lib::foo", 0));
    let p2 = place(&w.fake, range(&w, "lib::foo", 1));
    let pc = place(&w.fake, range(&w, "lib::foo(1, 2)", 0));
    let c = |key, place: &Place, why: &str| Changed {
        key,
        place: place.clone(),
        reason: why.as_bytes().to_vec(),
    };
    let mut expected = vec![
        c(U_MAIN, &p1, "renamed"),
        c(U_DERIVED1, &p1, "renamed"),
        c(U_DERIVED2, &p1, "renamed"),
        c(U_MAIN, &p2, "renamed"),
        c(U_K, &pc, "wrapped"),
    ];
    expected.sort();
    assert_eq!(out.changed, expected);
}

/// §7.5 rules 3 and 4: `expect_renamed` records the rule that undoes the
/// rename; twice with one name is once; with two names the second is
/// `invalid_argument`; each other refusal.
#[test]
fn s7_5_expect_renamed() {
    let w = world();
    let mut run = Run::new(&w, &w.fake);
    let foo = run.entity(w.foo);
    let bar = run.entity(w.bar);
    let r = run.reference(w.r1);
    let main = run.entity(w.main);
    assert_eq!(renamed(&mut run, foo, "foo2"), 0);
    assert_eq!(renamed(&mut run, foo, "foo2"), 0, "the same name again");
    assert_eq!(renamed(&mut run, foo, "foo3"), errc(Errc::InvalidArgument), "another name");
    assert_eq!(renamed(&mut run, r, "x"), errc(Errc::WrongKind));
    assert_eq!(renamed(&mut run, 0, "x"), errc(Errc::InvalidHandle));
    let ia = errc(Errc::InvalidArgument);
    for bad in ["", "1x", "a b", "a::b", "class", "\u{ff}x\u{1}"] {
        assert_eq!(renamed(&mut run, bar, bad), ia, "{bad:?}");
    }
    assert_eq!(run.status(wire::OP_EXPECT_RENAMED, &[bar], &[0xff]), ia, "not UTF-8");
    assert_eq!(renamed(&mut run, bar, "b$"), errc(Errc::Unsupported), "undecided");
    assert_eq!(renamed(&mut run, bar, "é"), errc(Errc::Unsupported), "undecided");
    assert_eq!(renamed(&mut run, main, "main2"), errc(Errc::Unsupported), "not keyed by name");
    assert_eq!(renamed(&mut run, bar, "bar"), ia, "its identifier already: no rename");
    assert_eq!(renamed(&mut run, foo, "foo"), ia, "the old name, after a rename");
    assert_eq!(renamed(&mut run, bar, "foo2"), 0, "a refused request did not take the entity");
    let out = run.finish();
    let rule = |from: &str, to: &str| Rule {
        parent: Parent::Unit(K_INL),
        kind: KeyKind::Function,
        name: Name::Identifier(from.as_bytes().to_vec()),
        disc: None,
        to_parent: Parent::Unit(K_INL),
        to_name: Name::Identifier(to.as_bytes().to_vec()),
    };
    // Two entities to one name in one scope: both recorded; verification
    // refuses it (§7.5 rule 3).
    assert_eq!(out.renamed, vec![rule("foo2", "bar"), rule("foo2", "foo")]);
}

/// §7.5 rule 3, §7.6 rule 1: of two names for one entity the first
/// succeeds and the second throws, in either order; the output is the
/// output of the run without the second call.
#[test]
fn s7_5_rule_3_the_first_name_wins_in_either_order() {
    let w = world();
    for (first, second) in [("x1", "x2"), ("x2", "x1")] {
        let mut both = Run::new(&w, &w.fake);
        let foo = both.entity(w.foo);
        assert_eq!(renamed(&mut both, foo, first), 0);
        assert_eq!(renamed(&mut both, foo, second), errc(Errc::InvalidArgument));
        let mut alone = Run::new(&w, &w.fake);
        let foo = alone.entity(w.foo);
        assert_eq!(renamed(&mut alone, foo, first), 0);
        let (both, alone) = (both.finish(), alone.finish());
        assert_eq!(both, alone, "{first} then {second}");
        assert_eq!(both.renamed.len(), 1);
        assert_eq!(both.renamed[0].name, Name::Identifier(first.as_bytes().to_vec()));
    }
}

// ---------------------------------------------------------------------------
// The statuses a subject may choose (WP-081 decision 17).

#[derive(Clone, Debug)]
enum Lie {
    None,
    EntityKey(Option<Digest>),
    Qualified(Option<Vec<Vec<u8>>>),
    UnitKeys(Result<Vec<Digest>, Errc>),
    RenameRule(Result<Rule, Errc>),
    Tiers(Option<[bool; 2]>),
    /// For a range the fake gives out: these answers, one per question, in
    /// turn (one range reached at tier 0 by one request and at tier 1 by
    /// another).
    TiersInTurn(Arc<Mutex<Vec<[bool; 2]>>>),
    Place(PlaceOf),
    ReferencesAt(Vec<Reference>),
}

/// A subject that delegates to the fake but for one lie.
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
    fn lookup_in(&self, scope: Entity, identifier: &[u8]) -> Option<Vec<Entity>> {
        self.fake.lookup_in(scope, identifier)
    }
    fn identifier(&self, v: Value) -> Option<Vec<u8>> {
        self.fake.identifier(v)
    }
    fn display(&self, v: Value) -> String {
        self.fake.display(v)
    }
    fn parent(&self, e: Entity) -> Option<Entity> {
        self.fake.parent(e)
    }
    fn parameters(&self, v: Value) -> Option<Vec<Entity>> {
        self.fake.parameters(v)
    }
    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc> {
        self.fake.declarations(e)
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
        self.fake.name_range(v)
    }
    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        self.fake.extent(v)
    }
    fn text(&self, r: Range) -> &[u8] {
        self.fake.text(r)
    }
    fn owned(&self, f: File) -> bool {
        self.fake.owned(f)
    }
    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc> {
        self.fake.calls_to(e)
    }
    fn callee(&self, x: Expression) -> Option<Entity> {
        self.fake.callee(x)
    }
    fn callee_reference(&self, x: Expression) -> Option<Reference> {
        self.fake.callee_reference(x)
    }
    fn arguments(&self, x: Expression) -> Option<Vec<Expression>> {
        self.fake.arguments(x)
    }
    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64> {
        self.fake.unresolved_named(identifier)
    }
    fn tiers(&self, r: Range) -> Option<[bool; 2]> {
        match &self.lie {
            Lie::Tiers(t) => *t,
            Lie::TiersInTurn(turns) => self.fake.tiers(r).map(|_| turns.lock().unwrap().remove(0)),
            _ => self.fake.tiers(r),
        }
    }
    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        if let Lie::Qualified(q) = &self.lie { q.clone() } else { self.fake.qualified(e) }
    }
    fn references_at(&self, r: Range) -> Vec<Reference> {
        if let Lie::ReferencesAt(x) = &self.lie { x.clone() } else { self.fake.references_at(r) }
    }
    fn references_named_at(&self, r: Range) -> Vec<Reference> {
        self.fake.references_named_at(r)
    }
    fn declarations_named_at(&self, r: Range) -> Vec<Declaration> {
        self.fake.declarations_named_at(r)
    }
    fn found_directly(&self, r: Reference) -> bool {
        self.fake.found_directly(r)
    }
    fn entity_key(&self, e: Entity) -> Option<Digest> {
        if let Lie::EntityKey(k) = self.lie { k } else { self.fake.entity_key(e) }
    }
    fn unit_keys(&self, v: Value) -> Result<Vec<Digest>, Errc> {
        if let Lie::UnitKeys(k) = &self.lie { k.clone() } else { self.fake.unit_keys(v) }
    }
    fn rename_rule(&self, e: Entity, new_identifier: &[u8]) -> Result<Rule, Errc> {
        if let Lie::RenameRule(r) = &self.lie {
            r.clone()
        } else {
            self.fake.rename_rule(e, new_identifier)
        }
    }
    fn place(&self, v: Value) -> PlaceOf {
        if let Lie::Place(p) = self.lie { p } else { self.fake.place(v) }
    }
    fn file_ref(&self, f: File) -> FileRef {
        self.fake.file_ref(f)
    }
}

/// What the operation named gives under a lie: `Ok(status)` or the host
/// failure. The handles are obtained under the lie too (the queries do
/// not ask the methods lied about).
fn under(w: &World, lie: Lie, what: &str) -> Result<u8, String> {
    let liar = Liar { fake: &w.fake, lie };
    let mut run = Run::new(w, &liar);
    let (op, ints, text): (u32, Vec<u64>, &[u8]) = match what {
        "spell" => (wire::OP_CODE_SPELL, vec![run.entity(w.bar)], b""),
        "replace" => {
            let e = run.extent(Value::Reference(w.r1));
            let s = run.spell(w.bar);
            (wire::OP_REPLACE, vec![e[0], e[1], s, 0], b"")
        }
        "report" => (wire::OP_REPORT, vec![1, run.entity(w.foo), 0, 0], b"m"),
        "changed" => (wire::OP_EXPECT_CHANGED, vec![run.reference(w.r1)], b"why"),
        "renamed" => (wire::OP_EXPECT_RENAMED, vec![run.entity(w.foo)], b"foo2"),
        _ => unreachable!("{what}"),
    };
    let mut out = FakeOut::with_capacity(ROOM, ROOM);
    let word = run.h.call(op, &ints, text, &mut out)?;
    assert_eq!(out.writes, 0);
    Ok(wire::status(word))
}

fn host_failure(r: Result<u8, String>, needles: &[&str]) {
    let m = r.expect_err("a host failure");
    for n in needles {
        assert!(m.contains(n), "{m:?} does not name {n:?}");
    }
}

/// `Subject::entity_key` (§7.1 rule 4): `None` is the one status it may
/// choose, and is `unsupported`; a key is recorded as the piece's.
#[test]
fn s7_1_subject_entity_key_statuses() {
    let w = world();
    assert_eq!(under(&w, Lie::EntityKey(None), "spell"), Ok(errc(Errc::Unsupported)));
    assert_eq!(under(&w, Lie::EntityKey(Some(Digest([9; 32]))), "spell"), Ok(0));
    assert_eq!(under(&w, Lie::None, "spell"), Ok(0));
}

/// `Subject::qualified` (§7.1 rule 4, §7.2 rule 2): `None` is
/// `unsupported`; an answer that is no qualified name of the entity (no
/// name, an empty name, a last name that is not the entity's identifier)
/// is a host failure naming the method.
#[test]
fn s7_2_subject_qualified_statuses() {
    let w = world();
    assert_eq!(under(&w, Lie::Qualified(None), "spell"), Ok(errc(Errc::Unsupported)));
    let names = |n: &[&str]| Some(n.iter().map(|s| s.as_bytes().to_vec()).collect());
    for lie in [names(&[]), names(&["lib", ""]), names(&["", "bar"]), names(&["lib", "baz"])] {
        host_failure(under(&w, Lie::Qualified(lie), "spell"), &["Subject::qualified"]);
    }
    assert_eq!(under(&w, Lie::Qualified(names(&["x", "bar"])), "spell"), Ok(0));
}

/// `Subject::unit_keys` (§7.5 rule 2): `unsupported` is passed on; any
/// other status, no key, or a key twice is a host failure naming the
/// method.
#[test]
fn s7_5_subject_unit_keys_statuses() {
    let w = world();
    assert_eq!(
        under(&w, Lie::UnitKeys(Err(Errc::Unsupported)), "changed"),
        Ok(errc(Errc::Unsupported))
    );
    for e in [Errc::InvalidHandle, Errc::WrongKind, Errc::InvalidArgument, Errc::LimitExceeded] {
        host_failure(
            under(&w, Lie::UnitKeys(Err(e)), "changed"),
            &["Subject::unit_keys", e.name()],
        );
    }
    host_failure(under(&w, Lie::UnitKeys(Ok(vec![])), "changed"), &["Subject::unit_keys"]);
    host_failure(under(&w, Lie::UnitKeys(Ok(vec![U_K, U_K])), "changed"), &["Subject::unit_keys"]);
}

/// `Subject::rename_rule` (§7.5 rule 3): `unsupported` is passed on; any
/// other status, or a rule that does not match the new identifier, is a
/// host failure naming the method.
#[test]
fn s7_5_subject_rename_rule_statuses() {
    let w = world();
    assert_eq!(
        under(&w, Lie::RenameRule(Err(Errc::Unsupported)), "renamed"),
        Ok(errc(Errc::Unsupported))
    );
    for e in [Errc::InvalidHandle, Errc::WrongKind, Errc::InvalidArgument, Errc::NotEditable] {
        host_failure(
            under(&w, Lie::RenameRule(Err(e)), "renamed"),
            &["Subject::rename_rule", e.name()],
        );
    }
    let wrong = w.fake.rename_rule(w.foo, b"other").expect("a rule");
    host_failure(under(&w, Lie::RenameRule(Ok(wrong)), "renamed"), &["Subject::rename_rule"]);
}

/// The other answers the effects check: the empty set of tiers, a
/// place outside its file, a reference at a range whose extent is not
/// that range.
#[test]
fn s7_subject_answers_the_effects_check() {
    let w = world();
    host_failure(under(&w, Lie::Tiers(Some([false, false])), "replace"), &["Subject::tiers"]);
    assert_eq!(under(&w, Lie::Tiers(None), "replace"), Ok(errc(Errc::InvalidHandle)));
    for t in [[true, false], [false, true], [true, true]] {
        assert_eq!(under(&w, Lie::Tiers(Some(t)), "replace"), Ok(0), "{t:?}");
    }
    let outside = Range { file: w.t, lo: 0, hi: 1 << 20 };
    host_failure(under(&w, Lie::Place(PlaceOf::Own(outside)), "report"), &["Subject::place"]);
    host_failure(
        under(&w, Lie::ReferencesAt(vec![w.r_indirect]), "replace"),
        &["Subject::references_at"],
    );
    host_failure(
        under(&w, Lie::ReferencesAt(vec![w.r1, w.r1]), "replace"),
        &["Subject::references_at"],
    );
}

// ---------------------------------------------------------------------------
// §7.6 rule 1: the tiers of an edit.

/// §7.6 `Edit.tiers`: a set of 0 and 1, never empty, bit `t` for tier
/// `t` (`spec/edits.md` §4.2), iterated in ascending order.
#[test]
fn s7_6_tiers_is_a_set() {
    assert_eq!(Tiers::from_reached([false, false]), None, "never empty");
    let sets =
        [(Tiers::ZERO, [true, false]), (Tiers::ONE, [false, true]), (Tiers::BOTH, [true, true])];
    for (t, reached) in sets {
        assert_eq!(Tiers::from_reached(reached), Some(t));
        assert_eq!([t.contains(0), t.contains(1)], reached);
        assert!(!t.contains(2));
        assert_eq!(
            t.iter().collect::<Vec<_>>(),
            (0..2).filter(|&i| reached[i as usize]).collect::<Vec<u8>>()
        );
        assert_eq!(t.union(t), t);
        assert_eq!(t.union(Tiers::BOTH), Tiers::BOTH);
    }
    assert_eq!([Tiers::ZERO.bits(), Tiers::ONE.bits(), Tiers::BOTH.bits()], [1, 2, 3]);
    assert_eq!(Tiers::ZERO.union(Tiers::ONE), Tiers::BOTH);
    assert_eq!(Tiers::ONE.union(Tiers::ZERO), Tiers::BOTH);
    assert_eq!(Tiers::BOTH.to_string(), "0,1");
}

/// Two requests of one run with one file, range and pieces, one reached
/// at tier 0 and one at tier 1, are one edit with tiers 0 and 1, in either
/// order: tiers are joined by union and are no part of the identity.
#[test]
fn s7_6_rule_1_tiers_are_joined_by_union() {
    let w = world();
    let e = range(&w, "lib::foo", 0);
    for turns in [[[true, false], [false, true]], [[false, true], [true, false]]] {
        let liar =
            Liar { fake: &w.fake, lie: Lie::TiersInTurn(Arc::new(Mutex::new(turns.to_vec()))) };
        let mut run = Run::new(&w, &liar);
        let r = run.extent(Value::Reference(w.r1));
        let x = run.text("x");
        run.replace(r, x, 0, "a");
        run.replace(r, x, 0, "b");
        let out = run.finish();
        assert_eq!(edits(&out), vec![(e.lo, e.hi, vec![lit("x")])], "{turns:?}: one edit");
        assert_eq!(out.edits[0].tiers, Tiers::BOTH, "{turns:?}");
        assert_eq!(out.edits[0].notes, vec![b"a".to_vec(), b"b".to_vec()]);
        assert!(out.is_canonical());
    }
    // Requests all at tier 1: tiers 1 (`s7_2_two_references_with_one_extent`
    // through the sibling rule; here a literal at the extent).
    let liar =
        Liar { fake: &w.fake, lie: Lie::TiersInTurn(Arc::new(Mutex::new(vec![[false, true]; 2]))) };
    let mut run = Run::new(&w, &liar);
    let r = run.extent(Value::Reference(w.r1));
    let x = run.text("x");
    run.replace(r, x, 0, "");
    run.replace(r, x, 0, "");
    assert_eq!(run.finish().edits[0].tiers, Tiers::ONE);
}

/// §7.6 rule 1: a range the subject reaches at both tiers at once (the
/// fake's `both`) is recorded with tiers 0 and 1, and an edit narrowed by
/// the sibling rule takes the tiers of the name range (WP-088 decision 5).
#[test]
fn s7_6_rule_1_a_range_reached_at_both_tiers() {
    let w = world();
    let e = range(&w, "lib::foo", 0);
    let mut f = w.fake.clone();
    f.both.push(name_in(e));
    let mut run = Run::new(&w, &f);
    let r = run.extent(Value::Reference(w.r1));
    let x = run.text("x");
    run.replace(r, x, 0, "");
    let s = run.spell(w.bar);
    run.replace(r, s, 0, "");
    let out = run.finish();
    let got: Vec<_> = out.edits.iter().map(|d| (d.place.lo, d.place.hi, d.tiers)).collect();
    assert_eq!(got, vec![(e.lo, e.hi, Tiers::ZERO), (e.hi - 3, e.hi, Tiers::BOTH)]);
}

// ---------------------------------------------------------------------------
// §7.6 The run output: independent of order and of handle numbers.

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

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A place a request names, by the value whose extent or name range it is.
#[derive(Clone, Copy, Debug)]
enum At {
    Extent(Value),
    Name(Value),
    /// The empty range at the start of the value's extent (a copy only).
    Empty(Value),
}

#[derive(Clone, Debug)]
enum PieceSpec {
    Text(&'static str),
    Copy(At),
    Spell(Entity),
}

#[derive(Clone, Debug)]
enum Request {
    Replace { at: At, pieces: Vec<PieceSpec>, group: Option<usize>, note: &'static str },
    Report { at: Option<Value>, severity: u64, message: &'static str },
    Changed { at: Value, why: &'static str },
    Renamed { entity: Entity },
}

fn random_requests(w: &World, rng: &mut Rng, n: usize) -> Vec<Request> {
    let editable = [
        At::Extent(Value::Reference(w.r1)),
        At::Name(Value::Reference(w.r1)),
        At::Extent(Value::Reference(w.r2)),
        At::Extent(Value::Reference(w.r_m1)),
        At::Extent(Value::Reference(w.r_indirect)),
        At::Extent(Value::Expression(w.call1)),
        At::Extent(Value::Expression(w.arg1)),
    ];
    let copyable = [
        At::Extent(Value::Reference(w.r_h)),
        At::Extent(Value::Expression(w.arg1)),
        At::Empty(Value::Reference(w.r1)),
        At::Empty(Value::Reference(w.r_h)),
    ];
    let spellable = [w.foo, w.bar, w.hidden, w.other_foo];
    let texts = ["", "x", "yy", "(", ")"];
    let values = [
        Value::Reference(w.r1),
        Value::Expression(w.call1),
        Value::Entity(w.foo),
        Value::Entity(w.other_foo),
    ];
    (0..n)
        .map(|_| match rng.below(6) {
            0..=2 => {
                let k = rng.below(4);
                let pieces = (0..k)
                    .map(|_| match rng.below(3) {
                        0 => PieceSpec::Text(texts[rng.below(texts.len())]),
                        1 => {
                            let all: Vec<At> = editable.iter().chain(&copyable).copied().collect();
                            PieceSpec::Copy(all[rng.below(all.len())])
                        }
                        _ => PieceSpec::Spell(spellable[rng.below(spellable.len())]),
                    })
                    .collect();
                Request::Replace {
                    at: editable[rng.below(editable.len())],
                    pieces,
                    group: if rng.below(2) == 0 { Some(rng.below(4)) } else { None },
                    note: ["", "n1", "n2"][rng.below(3)],
                }
            }
            3 => Request::Report {
                at: if rng.below(3) == 0 { None } else { Some(values[rng.below(values.len())]) },
                severity: rng.below(3) as u64,
                message: ["m1", "m2"][rng.below(2)],
            },
            4 => Request::Changed {
                at: [Value::Reference(w.r1), Value::Reference(w.r2), Value::Expression(w.call1)]
                    [rng.below(3)],
                why: ["a", "b"][rng.below(2)],
            },
            _ => Request::Renamed { entity: [w.foo, w.bar][rng.below(2)] },
        })
        .collect()
}

/// A run of requests, with the values it resolved.
struct Exec<'w, 's> {
    run: Run<'w, 's>,
    resolved: std::collections::BTreeMap<Value, u64>,
}

impl Exec<'_, '_> {
    fn handle(&mut self, v: Value) -> u64 {
        if let Some(&h) = self.resolved.get(&v) {
            return h;
        }
        let h = self.run.handle(v);
        self.resolved.insert(v, h);
        h
    }

    fn range(&mut self, at: At) -> [u64; 2] {
        let (op, v) = match at {
            At::Extent(v) | At::Empty(v) => (wire::OP_EXTENT_OF, v),
            At::Name(v) => (wire::OP_NAME_RANGE_OF, v),
        };
        let h = self.handle(v);
        let [file, sp] = self.run.range_of(op, h);
        match at {
            At::Empty(_) => [file, sp & !0xffff_ffff | sp >> 32],
            _ => [file, sp],
        }
    }
}

/// Runs the requests in the order given. `early`: resolve every value of
/// `values` (in that order) and make every group before the first
/// request; otherwise resolve and make each when first needed, and the
/// rest at the end, so that the same queries are made and the same groups
/// with other handle numbers.
fn run_requests(w: &World, requests: &[Request], values: &[Value], early: bool) -> RunOutput {
    let mut x = Exec { run: Run::new(w, &w.fake), resolved: std::collections::BTreeMap::new() };
    let mut groups: [Option<u64>; 4] = [None; 4];
    if early {
        for &v in values {
            x.handle(v);
        }
        for g in &mut groups {
            *g = Some(x.run.group());
        }
    }
    for r in requests {
        match r {
            Request::Replace { at, pieces, group, note } => {
                let mut codes = Vec::new();
                for p in pieces {
                    codes.push(match p {
                        PieceSpec::Text(s) => x.run.text(s),
                        PieceSpec::Copy(at) => {
                            let r = x.range(*at);
                            x.run.copy(r)
                        }
                        PieceSpec::Spell(e) => x.run.spell(*e),
                    });
                }
                let code = x.run.cat(&codes);
                let g = match group {
                    Some(i) => *groups[*i].get_or_insert_with(|| x.run.group()),
                    None => 0,
                };
                let target = x.range(*at);
                // §7.2 rule 1 at a name range (WP-132): a code with a
                // spelled piece there is one sibling's identifier or is
                // refused, whatever the order.
                let status = x.run.try_replace(target, code, g, note);
                let refused_at_a_name =
                    matches!(at, At::Name(_)) && status == errc(Errc::InvalidArgument);
                assert!(status == 0 || refused_at_a_name, "{status}");
            }
            Request::Report { at, severity, message } => {
                let ints = match at {
                    None => [0, 0, 0, *severity],
                    Some(v) => [1, x.handle(*v), 0, *severity],
                };
                assert_eq!(report(&mut x.run, ints, message), 0);
            }
            Request::Changed { at, why } => {
                let h = x.handle(*at);
                assert_eq!(changed(&mut x.run, h, why), 0);
            }
            Request::Renamed { entity } => {
                let h = x.run.entity(*entity);
                let name = if *entity == w.foo { "foo2" } else { "bar2" };
                assert_eq!(renamed(&mut x.run, h, name), 0);
            }
        }
    }
    for &v in values {
        x.handle(v);
    }
    for g in &mut groups {
        if g.is_none() {
            *g = Some(x.run.group());
        }
    }
    x.run.finish()
}

/// §7.6 rule 1: two hosts given the same requests in different orders,
/// with different handle numbers, finish with equal outputs, counters
/// included.
#[test]
fn s7_6_rule_1_the_output_is_independent_of_order_and_handles() {
    let w = world();
    let mut rng = Rng(0x7_6_1);
    let values = [
        Value::Reference(w.r1),
        Value::Reference(w.r2),
        Value::Reference(w.r_m1),
        Value::Reference(w.r_indirect),
        Value::Reference(w.r_h),
        Value::Expression(w.call1),
        Value::Expression(w.arg1),
        Value::Entity(w.foo),
        Value::Entity(w.other_foo),
    ];
    for round in 0..60 {
        let n = 1 + rng.below(30);
        let requests = random_requests(&w, &mut rng, n);
        let expected = run_requests(&w, &requests, &values, true);
        assert!(expected.is_canonical());
        // §7.1 rule 2, `spec/edits.md` §4.2: no copy piece is empty.
        assert!(no_empty_copy(&expected), "round {round}");
        let mut shuffled = requests.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, rng.below(i + 1));
        }
        let mut reversed = values.to_vec();
        reversed.reverse();
        assert_eq!(run_requests(&w, &shuffled, &reversed, true), expected, "round {round}");
        assert_eq!(run_requests(&w, &shuffled, &values, false), expected, "round {round}");
    }
}

// ---------------------------------------------------------------------------
// §7.7 Limits of a run.

/// §7.7: the defaults are the table of the specification.
#[test]
fn s7_7_the_defaults_are_the_table() {
    let spec = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/api.md"))
        .expect("spec/api.md");
    let l = Limits::default();
    for (row, value, field) in [
        ("| code values |", "2^22", l.code_values),
        ("| bytes of text in pieces, in total", "2^28", l.literal_bytes),
        ("| pieces, in total", "2^22", l.pieces),
        ("| pieces in one code value |", "2^16", l.pieces_per_code),
        ("| edits requested |", "2^20", l.edits),
        ("| groups |", "2^20", l.groups),
        ("| reports |", "2^16", l.reports),
        ("| declared changes", "2^20", l.changed),
        ("| bytes in one note, reason or message |", "2^12", l.text_bytes),
        ("| bytes of text arguments held, in total", "2^26", l.text_total),
        ("| bytes in one qualified name or identifier argument |", "2^12", l.name_bytes),
    ] {
        let line = spec
            .lines()
            .find(|x| x.starts_with(row))
            .unwrap_or_else(|| panic!("no row {row:?} in spec/api.md §7.7"));
        assert!(line.trim_end().ends_with(&format!("| {value} |")), "{line}");
        let exp: u32 = value[2..].parse().expect("2^n");
        assert_eq!(field, 1u64 << exp, "{row}");
    }
}

/// A run with small limits.
fn small(w: &World, f: impl FnOnce(&mut Limits)) -> Run<'_, '_> {
    let mut l = Limits::default();
    f(&mut l);
    Run::with_limits(w, &w.fake, l)
}

const LE: u8 = 6;

/// §7.7: each limit. At the limit the request is accepted; one more is
/// `limit_exceeded`.
#[test]
fn s7_7_each_limit_at_and_past() {
    let w = world();

    // Code values: text, copy, spell and cat each make one; the empty
    // code is none.
    let mut run = small(&w, |l| l.code_values = 4);
    let e = run.extent(Value::Reference(w.r1));
    let foo = run.entity(w.foo);
    let t = run.text("a");
    let c = run.copy(e);
    run.spell(w.bar);
    run.cat(&[t, c]);
    assert_eq!(run.status(wire::OP_CODE_TEXT, &[], b"b"), LE);
    assert_eq!(run.status(wire::OP_CODE_COPY, &e, &[]), LE);
    assert_eq!(run.status(wire::OP_CODE_SPELL, &[foo], &[]), LE);
    assert_eq!(run.status(wire::OP_CODE_CAT, &[t], &[]), LE);
    assert_eq!(run.text(""), 0);
    assert_eq!(run.cat(&[0]), 0);

    // Pieces in one code value: joined literals are one piece.
    let mut run = small(&w, |l| l.pieces_per_code = 2);
    let e = run.extent(Value::Reference(w.r1));
    let t = run.text("a");
    let c = run.copy(e);
    run.cat(&[c, t]);
    run.cat(&[t, t, t, t]);
    run.cat(&[t, 0, t, c]);
    assert_eq!(run.status(wire::OP_CODE_CAT, &[c, t, c], &[]), LE);
    assert_eq!(run.status(wire::OP_CODE_CAT, &[t, c, t], &[]), LE);

    // Pieces in total, in code values and in recorded edits.
    let mut run = small(&w, |l| l.pieces = 5);
    let e = run.extent(Value::Reference(w.r1));
    let e2 = run.extent(Value::Expression(w.arg1));
    let t = run.text("a");
    let c = run.copy(e);
    run.replace(e, c, 0, "");
    run.spell(w.bar);
    run.replace(e2, t, 0, "");
    assert_eq!(run.status(wire::OP_CODE_TEXT, &[], b"z"), LE);
    assert_eq!(run.try_replace(e, t, 0, ""), LE);
    run.replace(e, 0, 0, ""); // a deletion holds no piece

    // Bytes of text in pieces: literals and spelled text, in code values
    // and in recorded edits.
    let mut run = small(&w, |l| l.literal_bytes = 20);
    let e = run.extent(Value::Reference(w.r1));
    let name = run.name_range(Value::Reference(w.r1));
    let t = run.text("abc");
    let s = run.spell(w.bar); // "::lib::inl::bar": 15 bytes
    run.text("ab");
    assert_eq!(run.status(wire::OP_CODE_TEXT, &[], b"x"), LE);
    assert_eq!(run.status(wire::OP_CODE_CAT, &[t, t], &[]), LE);
    assert_eq!(run.try_replace(name, s, 0, ""), LE, "the text chosen, 3 bytes, counts again");
    let c = run.copy(e);
    run.replace(e, c, 0, "");

    // Edits requested: identical requests count.
    let mut run = small(&w, |l| l.edits = 2);
    let e = run.extent(Value::Reference(w.r1));
    run.replace(e, 0, 0, "");
    run.replace(e, 0, 0, "");
    assert_eq!(run.try_replace(e, 0, 0, ""), LE);

    // Groups.
    let mut run = small(&w, |l| l.groups = 2);
    run.group();
    run.group();
    assert_eq!(run.status(wire::OP_NEW_GROUP, &[], &[]), LE);

    // Reports.
    let mut run = small(&w, |l| l.reports = 2);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "a"), 0);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "a"), 0);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "a"), LE);

    // Distinct declared changes: a repeated declaration holds nothing more.
    let mut run = small(&w, |l| l.changed = 3);
    let r = run.reference(w.r1);
    let call = run.expression(w.call1);
    assert_eq!(changed(&mut run, r, "why"), 0, "three keys");
    assert_eq!(changed(&mut run, r, "why"), 0, "the same three");
    assert_eq!(changed(&mut run, call, "why"), LE);
    assert_eq!(changed(&mut run, r, "other"), LE);

    // Bytes in one note, reason or message.
    let mut run = small(&w, |l| l.text_bytes = 4);
    let e = run.extent(Value::Reference(w.r1));
    let r = run.reference(w.r1);
    run.replace(e, 0, 0, "abcd");
    assert_eq!(run.try_replace(e, 0, 0, "abcde"), LE);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "abcd"), 0);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "abcde"), LE);
    assert_eq!(changed(&mut run, r, "abcd"), 0);
    assert_eq!(changed(&mut run, r, "abcde"), LE);

    // Bytes in one identifier argument.
    // (The lookup of `lib::foo` is bound by it too: 8 bytes.)
    let mut run = small(&w, |l| l.name_bytes = 8);
    let foo = run.entity(w.foo);
    assert_eq!(renamed(&mut run, foo, "abcdefghi"), LE);
    assert_eq!(renamed(&mut run, foo, "abcdefgh"), 0);

    // Bytes of text arguments held: notes, messages, reasons (once per
    // declared change), new identifiers.
    let mut run = small(&w, |l| l.text_total = 6);
    let e = run.extent(Value::Reference(w.r1));
    let r = run.reference(w.r1);
    let foo = run.entity(w.foo);
    run.replace(e, 0, 0, "abc");
    assert_eq!(report(&mut run, [0, 0, 0, 0], "ab"), 0);
    assert_eq!(changed(&mut run, r, "a"), LE, "three keys, three bytes");
    assert_eq!(changed(&mut run, r, ""), 0);
    assert_eq!(renamed(&mut run, foo, "f"), 0);
    assert_eq!(report(&mut run, [0, 0, 0, 0], "x"), LE);
    assert_eq!(run.try_replace(e, 0, 0, "x"), LE);
}

// ---------------------------------------------------------------------------
// §4 rule 3: a failed request records nothing.

/// After any failed operation, `finish` equals the output of the same run
/// without it: random calls (most of them effects, many refused, some by
/// small limits) are replayed with only the ones that succeeded, and give
/// the same words and the same output.
#[test]
fn s4_a_failed_operation_leaves_the_output_as_without_it() {
    let w = world();
    let limits = Limits {
        code_values: 24,
        literal_bytes: 96,
        pieces: 40,
        pieces_per_code: 4,
        edits: 16,
        groups: 3,
        reports: 6,
        changed: 5,
        text_bytes: 3,
        text_total: 40,
        name_bytes: 10,
        ..Limits::default()
    };
    let mut rng = Rng(0x4_3);
    let texts: [&[u8]; 8] = [b"", b"x", b"yy", b"abcd", b"lib::foo", b"foo2", b"1x", b"\xff"];
    for round in 0..40 {
        let mut a = Host::new(&w.fake, Digest([1; 32]), limits);
        let mut pool: Vec<u64> = vec![0, 1, 2, u64::MAX];
        let mut log: Vec<(u32, Vec<u64>, Vec<u8>, u64)> = Vec::new();
        let mut failed = 0;
        for _ in 0..400 {
            let op = match rng.below(10) {
                0 => wire::OP_LOOKUP,
                1 => [wire::OP_REFERENCES_TO, wire::OP_CALLS_TO, wire::OP_ARGUMENTS_AS_WRITTEN]
                    [rng.below(3)],
                2 => [wire::OP_EXTENT_OF, wire::OP_NAME_RANGE_OF][rng.below(2)],
                _ => {
                    let effects: Vec<u32> =
                        wire::OPERATIONS.iter().filter(|o| o.effect).map(|o| o.op).collect();
                    effects[rng.below(effects.len())]
                }
            };
            let n = match wire::operation(op).map(|o| o.ints) {
                Some(wire::Arity::Exactly(n)) => n,
                _ => rng.below(5),
            };
            let ints: Vec<u64> = (0..n)
                .map(|i| match rng.below(8) {
                    0 => rng.below(4) as u64,
                    1 => rng.next(),
                    // The second integer of a range: a span of the pool or a small one.
                    2 if i % 2 == 1 => (rng.below(80) as u64) << 32 | rng.below(90) as u64,
                    _ => pool[rng.below(pool.len())],
                })
                .collect();
            let bytes: Vec<u8> = match op {
                wire::OP_LOOKUP => {
                    [&b"lib::foo"[..], b"lib::bar", b"other::foo", b"hidden", b"lib"][rng.below(5)]
                        .to_vec()
                }
                _ if wire::operation(op).is_some_and(|o| o.text) => {
                    texts[rng.below(texts.len())].to_vec()
                }
                _ => Vec::new(),
            };
            let mut out = FakeOut::with_capacity(ROOM, ROOM);
            let word = a
                .call(op, &ints, &bytes, &mut out)
                .unwrap_or_else(|e| panic!("round {round}: op {op} {ints:x?}: host failure {e}"));
            if wire::status(word) == 0 {
                pool.extend(&out.ints);
                if wire::operation(op).is_some_and(|o| o.convention == wire::Convention::V) {
                    pool.push(wire::payload(word));
                }
                log.push((op, ints, bytes, word));
            } else {
                failed += 1;
            }
        }
        assert!(failed > 20, "round {round}: the calls exercise refusals ({failed})");
        let mut b = Host::new(&w.fake, Digest([1; 32]), limits);
        for (op, ints, bytes, word) in &log {
            let mut out = FakeOut::with_capacity(ROOM, ROOM);
            assert_eq!(b.call(*op, ints, bytes, &mut out), Ok(*word), "round {round}: op {op}");
        }
        assert_eq!(b.failure_count(), 0);
        let out = a.finish();
        assert!(out.is_canonical());
        assert!(no_empty_copy(&out), "round {round}: §7.1 rule 2");
        assert_eq!(out, b.finish(), "round {round}");
    }
}

// ---------------------------------------------------------------------------
// The rendering of an output (WP-088 Interface).

#[test]
fn s7_6_render() {
    let file = FileRef { path: b"/src/a b.cpp".to_vec(), size: 9, content: Digest([3; 32]) };
    let at = |lo, hi| Place { file: Arc::new(file.clone()), lo, hi };
    let mut out = RunOutput::empty(Digest([0xab; 32]));
    out.edits = vec![
        Edit {
            place: at(1, 4),
            pieces: vec![
                lit("g(\"\\\n\t\u{1}"),
                Piece::Copy(at(5, 6)),
                spelled(Digest([0xcd; 32]), "::f"),
            ],
            tiers: Tiers::BOTH,
            notes: vec![b"n1".to_vec(), b"n2".to_vec()],
        },
        Edit { place: at(7, 8), pieces: vec![], tiers: Tiers::ZERO, notes: vec![] },
        Edit { place: at(8, 9), pieces: vec![], tiers: Tiers::ONE, notes: vec![] },
    ];
    out.groups = vec![vec![0, 1]];
    out.reports = vec![
        Report { place: None, severity: Severity::Note, message: b"m".to_vec(), attached: false },
        Report {
            place: Some(at(0, 1)),
            severity: Severity::Error,
            message: b"e".to_vec(),
            attached: true,
        },
    ];
    out.changed =
        vec![Changed { key: Digest([0x11; 32]), place: at(2, 3), reason: b"why".to_vec() }];
    out.renamed = vec![Rule {
        parent: Parent::Unit(Digest([0x22; 32])),
        kind: KeyKind::Function,
        name: Name::Identifier(b"g".to_vec()),
        disc: Some(Digest([0x33; 32])),
        to_parent: Parent::Root,
        to_name: Name::Identifier(b"f".to_vec()),
    }];
    out.counters = vec![slip_model::Counter { name: "api.edits".to_owned(), value: 2 }];
    let mut text = String::new();
    out.render(&mut text).expect("render");
    let d = |b: u8| Digest([b; 32]).to_string();
    let expected = format!(
        "transform {}\n\
         edit 0 /src/a b.cpp:1-4 tiers 0,1 = \"g(\\\"\\\\\\n\\t\\x01\" copy(/src/a b.cpp:5-6) spell({} \"::f\") note \"n1\" note \"n2\"\n\
         edit 1 /src/a b.cpp:7-8 tiers 0 =\n\
         edit 2 /src/a b.cpp:8-9 tiers 1 =\n\
         group 0 1\n\
         report - note \"m\"\n\
         report /src/a b.cpp:0-1 error \"e\" attached\n\
         changed {} /src/a b.cpp:2-3 \"why\"\n\
         renamed function {} \"g\" disc {} -> root \"f\"\n\
         counter api.edits 2\n",
        d(0xab),
        d(0xcd),
        d(0x11),
        d(0x22),
        d(0x33),
    );
    assert_eq!(text, expected);
}

/// The rendering of every form of `Name` (`spec/digest-kinds.md` §3.2):
/// a written form for each, none of them `Debug`'s.
#[test]
fn s7_6_render_names() {
    let d = Digest([0x44; 32]);
    let forms = [
        (Name::Identifier(b"f".to_vec()), "\"f\"".to_owned()),
        (Name::Operator(3), "operator(3)".to_owned()),
        (Name::Constructor, "constructor".to_owned()),
        (Name::Destructor, "destructor".to_owned()),
        (Name::Conversion(d), format!("conversion({d})")),
        (Name::LiteralOperator(b"_km".to_vec()), "literal_operator(\"_km\")".to_owned()),
        (Name::DeductionGuide, "deduction_guide".to_owned()),
        (Name::Anonymous, "anonymous".to_owned()),
        (Name::Content, "content".to_owned()),
        (Name::Local(2), "local(2)".to_owned()),
        (
            Name::UnnamedBy(Box::new(Name::Identifier(b"t".to_vec()))),
            "unnamed_by(\"t\")".to_owned(),
        ),
        (Name::UnnamedOrdinal(1), "unnamed_ordinal(1)".to_owned()),
        (Name::Property(7), "property(7)".to_owned()),
        (Name::Order, "order".to_owned()),
        (Name::Parameter(0), "parameter(0)".to_owned()),
    ];
    for (name, written) in forms {
        let mut out = RunOutput::empty(Digest([0; 32]));
        out.renamed = vec![Rule {
            parent: Parent::Builtin,
            kind: KeyKind::Function,
            name: Name::Identifier(b"g".to_vec()),
            disc: None,
            to_parent: Parent::CLinkage,
            to_name: name,
        }];
        let mut text = String::new();
        out.render(&mut text).expect("render");
        let line = text.lines().nth(1).expect("the rule");
        assert_eq!(line, format!("renamed function builtin \"g\" -> c_linkage {written}"));
    }
}
