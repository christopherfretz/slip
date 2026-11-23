//! The transform request `TRFM` (`spec/edits.md` §2.1, §3) and the `edits`
//! section of a map shard (`spec/edits.md` §4, section 17 of
//! `spec/model.md` §8), in binary and JSON form. Work package WP-060; the
//! offsets, paths and JSON forms pinned here are
//! `spec/landed/edits/WP-060.md`. Each test names the section it covers.

use std::fs;
use std::path::Path;

use slip_model::container::{TRAILER_LEN, Writer};
use slip_model::edits::{
    Changed, Claim, Edits, Failure, FailureKind, HostRefusal, Link, MacroSite, Piece, Place,
    Reason, Report, Role, SCHEME, Severity, SourcePlace, Stance, Subject, subject_flags, tiers,
};
use slip_model::formats::{JsonFormError, content_id, dump, undump};
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::keys::{KeyKind, Name, Parent, Rule};
use slip_model::transform::{
    Ownership, Rename, Script, TargetKind, Transform, TransformKind, covers, is_canonical,
    is_file_path,
};
use slip_model::wire::{Encoder, Error, ErrorKind};
use slip_model::{Counter, Digest, FileRef, Outcome, Phase, Shard, Status};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn file(path: &[u8], size: u64, n: u8) -> FileRef {
    FileRef { path: path.to_vec(), size, content: d(n) }
}

fn place(file: u64, lo: u64, hi: u64) -> Place {
    Place { file, lo, hi }
}

fn ok() -> Outcome {
    Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() }
}

fn hex(parts: &[&str]) -> Vec<u8> {
    let text: String = parts.concat().chars().filter(|c| !c.is_whitespace()).collect();
    assert!(text.len().is_multiple_of(2), "odd hex in test");
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn rep(byte: &str, n: usize) -> String {
    byte.repeat(n)
}

fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let body = bytes.len() - TRAILER_LEN;
    let t = content_digest(&bytes[..body]);
    bytes[body..].copy_from_slice(t.as_bytes());
    bytes
}

fn patch(bytes: &[u8], at: usize, from: u8, to: u8) -> Vec<u8> {
    let mut b = bytes.to_vec();
    assert_eq!(b[at], from, "patch: byte {at} is not what the test expects");
    b[at] = to;
    reseal(b)
}

fn parse(text: &str) -> Value {
    json::parse(text.as_bytes()).unwrap()
}

fn set(v: &mut Value, path: &[&str], new: Value) {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &mut m.iter_mut().find(|(k, _)| k == step).unwrap().1,
            Value::Array(a) => &mut a[step.parse::<usize>().unwrap()],
            _ => panic!("set: no {step}"),
        };
    }
    *cur = new;
}

fn error(r: Result<impl std::fmt::Debug, Error>) -> (ErrorKind, usize) {
    let e = r.expect_err("expected an error");
    (e.kind, e.offset)
}

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

/// The file offset of the payload of section `id`, read from the framing.
fn payload_at(bytes: &[u8], id: u64) -> usize {
    fn uv(bytes: &[u8], at: &mut usize) -> u64 {
        let (mut v, mut shift) = (0u64, 0);
        loop {
            let b = bytes[*at];
            *at += 1;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return v;
            }
            shift += 7;
        }
    }
    let mut at = 16;
    loop {
        let this = uv(bytes, &mut at);
        assert_ne!(this, 0, "no section {id}");
        let len = uv(bytes, &mut at) as usize;
        if this == id {
            return at;
        }
        at += len;
    }
}

// ---------------------------------------------------------------------------
// A second, independent encoder (spec/edits.md §3, §4; spec/model.md §2),
// used to build files that break an invariant (`to_bytes` debug-asserts
// validity), to cross-check `to_bytes`, and to find the offset of every
// field an invariant names: `marks` maps the JSON path of a field or
// element to the offset of its first byte in the payload.

struct Raw {
    e: Encoder,
    marks: Vec<(String, usize)>,
}

impl Raw {
    fn new() -> Raw {
        Raw { e: Encoder::new(), marks: Vec::new() }
    }

    fn mark(&mut self, path: String) {
        let at = self.e.as_bytes().len();
        self.marks.push((path, at));
    }

    fn opt<T>(&mut self, v: Option<&T>, f: impl FnOnce(&mut Raw, &T)) {
        match v {
            None => {
                self.e.u8(0);
            }
            Some(v) => {
                self.e.u8(1);
                f(self, v);
            }
        }
    }

    fn place(&mut self, p: &Place) {
        self.e.uv(p.file).uv(p.lo).uv(p.hi);
    }

    fn rule(&mut self, r: &Rule) {
        r.encode(&mut self.e);
    }
}

#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §4")]
fn raw_edits(x: &Edits) -> Raw {
    const S: &str = "sections.edits";
    let mut r = Raw::new();
    r.mark(S.to_owned());
    r.mark(format!("{S}.scheme"));
    r.e.uv(x.scheme).digest(&x.transform);
    r.opt(x.failure.as_ref(), |r, f| {
        r.e.uv(f.kind as u64).str(&f.message);
        r.opt(f.place.as_ref(), |r, p| {
            r.e.bytes(&p.file).uv(p.line).uv(p.column);
        });
        r.e.str(&f.rendered);
        r.opt(f.refusal.as_ref(), |r, h| {
            r.e.str(&h.function).str(&h.errc).str(&h.message);
            r.mark("sections.edits.failure.refusal.subject".to_owned());
            r.opt(h.subject.as_ref(), Raw::place);
        });
    });
    r.mark(format!("{S}.files"));
    r.e.count(x.files.len());
    for (i, f) in x.files.iter().enumerate() {
        r.mark(format!("{S}.files[{i}]"));
        r.e.bytes(&f.path).uv(f.size).digest(&f.content);
    }
    r.mark(format!("{S}.subjects"));
    r.e.count(x.subjects.len());
    for (i, s) in x.subjects.iter().enumerate() {
        r.mark(format!("{S}.subjects[{i}]"));
        r.e.digest(&s.id);
        r.mark(format!("{S}.subjects[{i}].rule"));
        r.opt(s.rule.as_ref(), Raw::rule);
        r.e.uv(s.role as u64);
        r.mark(format!("{S}.subjects[{i}].of"));
        r.e.count(s.of.len());
        for k in &s.of {
            r.e.digest(k);
        }
        r.mark(format!("{S}.subjects[{i}].flags"));
        r.e.uv(s.flags).str(&s.display);
    }
    r.mark(format!("{S}.links"));
    r.e.count(x.links.len());
    for (i, l) in x.links.iter().enumerate() {
        r.mark(format!("{S}.links[{i}]"));
        r.e.digest(&l.a).digest(&l.b);
    }
    r.mark(format!("{S}.claims"));
    r.e.count(x.claims.len());
    for (i, c) in x.claims.iter().enumerate() {
        let at = format!("{S}.claims[{i}]");
        r.mark(at.clone());
        r.e.uv(c.file).uv(c.lo).uv(c.hi).uv(c.stance as u64);
        r.mark(format!("{at}.pieces"));
        r.e.count(c.pieces.len());
        for (j, p) in c.pieces.iter().enumerate() {
            r.mark(format!("{at}.pieces[{j}]"));
            match p {
                Piece::Literal(b) => {
                    r.e.uv(0).bytes(b);
                }
                Piece::Copy(p) => {
                    r.e.uv(1);
                    r.place(p);
                }
                Piece::Spelled { key, text } => {
                    r.e.uv(2).digest(key).bytes(text);
                }
                Piece::Identifier { text, key } => {
                    r.e.uv(3).bytes(text);
                    match key {
                        None => r.e.u8(0),
                        Some(k) => r.e.u8(1).digest(k),
                    };
                }
            }
        }
        r.mark(format!("{at}.subjects"));
        r.e.count(c.subjects.len());
        for k in &c.subjects {
            r.e.digest(k);
        }
        r.mark(format!("{at}.reason"));
        r.e.uv(c.reason as u64);
        r.mark(format!("{at}.tiers"));
        r.e.uv(c.tiers);
        r.opt(c.macro_site.as_ref(), |r, m| {
            r.e.uv(m.name_start).bytes(&m.name);
        });
        r.mark(format!("{at}.votes"));
        r.e.uv(c.votes);
        r.mark(format!("{at}.notes"));
        r.e.count(c.notes.len());
        for n in &c.notes {
            r.e.bytes(n);
        }
    }
    r.mark(format!("{S}.groups"));
    r.e.count(x.groups.len());
    for (i, g) in x.groups.iter().enumerate() {
        r.mark(format!("{S}.groups[{i}]"));
        r.e.count(g.len());
        for &m in g {
            r.e.uv(m);
        }
    }
    r.mark(format!("{S}.reports"));
    r.e.count(x.reports.len());
    for (i, rep) in x.reports.iter().enumerate() {
        r.mark(format!("{S}.reports[{i}]"));
        r.opt(rep.place.as_ref(), Raw::place);
        r.e.uv(rep.severity as u64).bytes(&rep.message).bool(rep.attached);
    }
    r.mark(format!("{S}.changed"));
    r.e.count(x.changed.len());
    for (i, c) in x.changed.iter().enumerate() {
        r.mark(format!("{S}.changed[{i}]"));
        r.e.digest(&c.key);
        r.place(&c.place);
        r.e.bytes(&c.reason);
    }
    r.mark(format!("{S}.renamed"));
    r.e.count(x.renamed.len());
    for (i, rule) in x.renamed.iter().enumerate() {
        r.mark(format!("{S}.renamed[{i}]"));
        r.rule(rule);
    }
    r
}

/// A shard with header, outcome, inputs, counters and edits only.
fn raw(s: &Shard) -> Vec<u8> {
    assert!(s.digests.is_none() && s.verdict.is_none() && s.binding.is_none());
    assert!(s.rule_use.is_none() && s.spellings.is_none());
    let mut w = Writer::new(*b"SHRD", 1);
    let mut e = Encoder::new();
    e.digest(&s.observer).uv(s.phase as u64).uv(s.round);
    w.section(1, e.as_bytes());
    let mut e = Encoder::new();
    e.uv(s.outcome.status as u64)
        .uv(s.outcome.errors)
        .uv(s.outcome.warnings)
        .str(&s.outcome.message);
    w.section(2, e.as_bytes());
    if let Some(inputs) = &s.inputs {
        let mut e = Encoder::new();
        e.count(inputs.len());
        for f in inputs {
            e.bytes(&f.path).uv(f.size).digest(&f.content);
        }
        w.section(3, e.as_bytes());
    }
    let mut e = Encoder::new();
    e.count(s.counters.len());
    for c in &s.counters {
        e.str(&c.name).uv(c.value);
    }
    w.section(4, e.as_bytes());
    if let Some(x) = &s.edits {
        w.section(17, raw_edits(x).e.as_bytes());
    }
    w.finish()
}

/// The file offset of the field at `path` in `raw(s)`.
fn at(s: &Shard, path: &str) -> usize {
    let bytes = raw(s);
    let base = payload_at(&bytes, 17);
    let r = raw_edits(s.edits.as_ref().unwrap());
    let (_, off) = r
        .marks
        .iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("the raw encoder marks no {path}"));
    base + off
}

// ---------------------------------------------------------------------------
// Samples

fn rule(parent: Parent, from: &[u8], to: &[u8]) -> Rule {
    Rule {
        parent,
        kind: KeyKind::Function,
        name: Name::Identifier(from.to_vec()),
        disc: None,
        to_parent: parent,
        to_name: Name::Identifier(to.to_vec()),
    }
}

fn claim(file: u64, lo: u64, hi: u64, stance: Stance) -> Claim {
    Claim {
        file,
        lo,
        hi,
        stance,
        pieces: Vec::new(),
        subjects: Vec::new(),
        reason: if stance == Stance::Refuse { Reason::Crossing } else { Reason::None },
        tiers: tiers::T0,
        macro_site: None,
        votes: 1,
        notes: Vec::new(),
    }
}

fn literal(b: &[u8]) -> Piece {
    Piece::Literal(b.to_vec())
}

#[allow(clippy::unnecessary_wraps, reason = "the value of the opt field `macro`")]
fn macro_site(name_start: u64, name: &[u8]) -> Option<MacroSite> {
    Some(MacroSite { name_start, name: name.to_vec() })
}

/// An `edits` section with every record at least once: three files (one
/// path not UTF-8), subjects of every role, a link, a replace claim with
/// each piece, a replace through a macro argument with two votes, a keep,
/// a refuse in a macro body, a refuse in a read-only file, an unresolved
/// claim, a group, reports with and without a place, a `changed` entry and
/// two rules.
fn sample() -> Edits {
    Edits {
        scheme: SCHEME,
        transform: d(0x44),
        failure: None,
        files: vec![file(b"/s/a.cc", 100, 1), file(b"/s/a.h", 60, 2), file(b"/s/\xffb.h", 40, 3)],
        subjects: vec![
            Subject {
                id: d(0x10),
                rule: Some(rule(Parent::Root, b"f", b"g")),
                role: Role::Seed,
                of: Vec::new(),
                flags: 0,
                display: "n::f".to_owned(),
            },
            Subject {
                id: d(0x11),
                rule: None,
                role: Role::Follower,
                of: vec![d(0x10)],
                flags: subject_flags::READ_ONLY | subject_flags::NAME_TAKEN,
                display: "n::f<int>".to_owned(),
            },
            Subject {
                id: d(0x12),
                rule: None,
                role: Role::Candidate,
                of: Vec::new(),
                flags: subject_flags::UNREACHABLE | subject_flags::UNSUPPORTED,
                display: "m::f".to_owned(),
            },
        ],
        links: vec![Link { a: d(0x10), b: d(0x12) }],
        claims: vec![
            Claim {
                pieces: vec![
                    literal(b"g"),
                    Piece::Copy(place(1, 0, 4)),
                    Piece::Spelled { key: d(0x10), text: b"::n::g".to_vec() },
                ],
                subjects: vec![d(0x10)],
                notes: vec![b"note a".to_vec(), b"note \xff".to_vec()],
                ..claim(0, 10, 11, Stance::Replace)
            },
            Claim {
                pieces: vec![literal(b"g")],
                subjects: vec![d(0x10), d(0x11)],
                tiers: tiers::T0 | tiers::T1,
                macro_site: macro_site(15, b"ID"),
                votes: 2,
                ..claim(0, 20, 21, Stance::Replace)
            },
            claim(0, 30, 31, Stance::Keep),
            Claim {
                subjects: vec![d(0x12)],
                reason: Reason::MacroBody,
                tiers: tiers::T2,
                macro_site: macro_site(2, b"M"),
                ..claim(1, 5, 6, Stance::Refuse)
            },
            Claim {
                subjects: vec![d(0x10)],
                reason: Reason::NotOwned,
                ..claim(2, 1, 2, Stance::Refuse)
            },
            Claim {
                subjects: vec![d(0x12)],
                tiers: tiers::T1,
                macro_site: macro_site(1, b"X"),
                ..claim(2, 3, 4, Stance::Unresolved)
            },
        ],
        groups: vec![vec![0, 1]],
        reports: vec![
            Report {
                place: None,
                severity: Severity::Review,
                message: b"whole unit".to_vec(),
                attached: false,
            },
            Report {
                place: Some(place(0, 10, 11)),
                severity: Severity::Note,
                message: b"\xffbytes".to_vec(),
                attached: true,
            },
            Report {
                place: Some(place(0, 10, 11)),
                severity: Severity::Error,
                message: b"e".to_vec(),
                attached: false,
            },
        ],
        changed: vec![Changed {
            key: d(0x20),
            place: place(0, 10, 11),
            reason: b"renamed callee".to_vec(),
        }],
        renamed: vec![rule(Parent::Root, b"f", b"g"), rule(Parent::Unit(d(5)), b"h", b"i")],
    }
}

/// A failed run (§4.6): an uncaught `slip::error` whose subject is in the
/// one file the section names.
fn failed_run() -> Edits {
    Edits {
        scheme: SCHEME,
        transform: d(0x44),
        failure: Some(Failure {
            kind: FailureKind::Uncaught,
            message: "uncaught slip::error".to_owned(),
            place: Some(SourcePlace { file: b"/t/x.cc".to_vec(), line: 3, column: 7 }),
            rendered: "x.cc:3:7: error: uncaught\n  in f()".to_owned(),
            refusal: Some(HostRefusal {
                function: "slip::replace".to_owned(),
                errc: "not_editable".to_owned(),
                message: "the range is in a macro body".to_owned(),
                subject: Some(place(0, 1, 2)),
            }),
        }),
        files: vec![file(b"/s/a.cc", 100, 1)],
        subjects: Vec::new(),
        links: Vec::new(),
        claims: Vec::new(),
        groups: Vec::new(),
        reports: Vec::new(),
        changed: Vec::new(),
        renamed: Vec::new(),
    }
}

fn empty_edits() -> Edits {
    Edits { failure: None, files: Vec::new(), ..failed_run() }
}

fn map_shard(edits: Edits) -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 0,
        outcome: ok(),
        inputs: Some(vec![file(b"/s/a.cc", 100, 1), file(b"/s/a.h", 60, 2)]),
        counters: vec![Counter { name: "map.claims".to_owned(), value: 6 }],
        digests: None,
        verdict: None,
        edits: Some(edits),
        binding: None,
        rule_use: None,
        spellings: None,
    }
}

fn round_trip(s: &Shard) {
    s.validate().unwrap();
    let bytes = s.to_bytes();
    assert_eq!(bytes, raw(s), "to_bytes agrees with the test's encoder");
    let back = Shard::from_bytes(&bytes).unwrap();
    assert_eq!(&back, s);
    let text = json::write_pretty(&s.to_json()).unwrap();
    let from_text = Shard::from_json(&parse(&text)).unwrap();
    assert_eq!(&from_text, s);
    assert_eq!(from_text.to_bytes(), bytes, "binary → JSON → binary");
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[bytes.len() - 32..]);
}

// ---------------------------------------------------------------------------
// The edits section, byte for byte (spec/edits.md §4, spec/model.md §2, §8)

/// A map shard: observer 33…, round 0, ok, no counters, and an edits
/// section for request 44… naming "/a" (10 bytes, 55…) with one replace
/// claim of [2, 5) whose pieces are "xy", a copy of [0, 1) and "n" spelled
/// for 66…, with the note "k"; and a note report "m" without a place.
fn small() -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 0,
        outcome: ok(),
        inputs: None,
        counters: Vec::new(),
        digests: None,
        verdict: None,
        edits: Some(Edits {
            files: vec![file(b"/a", 10, 0x55)],
            claims: vec![Claim {
                pieces: vec![
                    literal(b"xy"),
                    Piece::Copy(place(0, 0, 1)),
                    Piece::Spelled { key: d(0x66), text: b"n".to_vec() },
                ],
                notes: vec![b"k".to_vec()],
                ..claim(0, 2, 5, Stance::Replace)
            }],
            reports: vec![Report {
                place: None,
                severity: Severity::Note,
                message: b"m".to_vec(),
                attached: false,
            }],
            ..empty_edits()
        }),
        binding: None,
        rule_use: None,
        spellings: None,
    }
}

const SMALL_TRAILER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn small_bytes() -> Vec<u8> {
    reseal(hex(&[
        "53 4c 49 50 1a 0a 00 00  53 48 52 44  01 00 00 00", // magic, SHRD, version 1
        "01 22",
        &rep("33", 32),
        "01 00",              // header: observer, map, round 0
        "02 04  00 00 00 00", // outcome: ok, 0, 0, ""
        "04 01  00",          // counters: []
        "11 8b 01",           // edits, 139 bytes:
        "01",                 //   scheme 1                       @64
        &rep("44", 32),       //   transform
        "00",                 //   no failure                     @97
        "01  02 2f 61  0a",   //   files: "/a", 10 bytes          @98
        &rep("55", 32),
        "00  00",      //   no subjects, no links          @135
        "01",          //   claims: 1                      @137
        "00 02 05 00", //     file 0, [2, 5), replace  @138
        "03",          //     pieces: 3                @142
        "00 02 78 79", //       literal "xy"           @143
        "01 00 00 01", //       copy (0, 0, 1)         @147
        "02",          //       spelled                @151
        &rep("66", 32),
        "01 6e",          //         "n"
        "00",             //     subjects []                @186
        "00",             //     reason none                @187
        "01",             //     tiers {0}                  @188
        "00",             //     no macro
        "01",             //     votes 1                    @190
        "01 01 6b",       //     notes ["k"]                @191
        "00",             //   groups []                    @194
        "01",             //   reports: 1
        "00 00 01 6d 00", //  no place, note, "m", false   @196
        "00 00",          //   changed [], renamed []       @201
        "00",             // end                            @203
        SMALL_TRAILER,    // the BLAKE3 of the rest, written by reseal
    ]))
}

// spec/edits.md §4, spec/model.md §2, §5, §8: a map shard with an edits
// section, byte for byte.
#[test]
fn edits_bytes_exact() {
    let bytes = small_bytes();
    assert_eq!(bytes.len(), 236);
    assert_eq!(small().to_bytes(), bytes);
    assert_eq!(raw(&small()), bytes);
    assert_eq!(Shard::from_bytes(&bytes).unwrap(), small());
}

// spec/model.md §10, spec/landed/edits/WP-060.md §2: the rendering of the
// sample and of a failed run, pinned.
#[test]
fn golden_shard_with_edits() {
    for (name, s) in [
        ("shard-edits.json", map_shard(sample())),
        ("shard-edits-failure.json", map_shard(failed_run())),
    ] {
        let bytes = s.to_bytes();
        let text = dump(&bytes).unwrap();
        golden(name, &text);
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        assert_eq!(Shard::from_json(&parse(&text)).unwrap(), s);
    }
}

// spec/edits.md §4, §4.6, spec/model.md §10: binary and JSON round trips of
// a section with every record, of a failed run, and of an empty section.
#[test]
fn edits_round_trip() {
    round_trip(&map_shard(sample()));
    round_trip(&map_shard(failed_run()));
    round_trip(&map_shard(empty_edits()));
    round_trip(&small());
    // A failed run that names no file, with nothing optional present.
    let mut x = failed_run();
    x.files.clear();
    let f = x.failure.as_mut().unwrap();
    f.place = None;
    f.refusal = None;
    f.kind = FailureKind::StepBudget;
    round_trip(&map_shard(x));
    // Values at and above 2^53 are decimal strings in JSON.
    let mut x = sample();
    x.files[2].size = u64::MAX;
    x.claims[5].lo = 1 << 53;
    x.claims[5].hi = u64::MAX;
    x.claims[1].votes = u64::MAX;
    round_trip(&map_shard(x));
    // The sample is already normal: normalising changes nothing.
    let mut x = sample();
    x.normalise();
    assert_eq!(x, sample());
}

// spec/edits.md §4.2, spec/landed/edits/WP-060.md §2: the JSON form of a
// piece is a one-member object; anything else is an error at its path.
#[test]
fn piece_json() {
    let v = map_shard(sample()).to_json();
    let pieces =
        v.get("sections").unwrap().get("edits").unwrap().get("claims").unwrap().as_array().unwrap()
            [0]
        .get("pieces")
        .unwrap()
        .clone();
    assert_eq!(
        json::write_compact(&pieces).unwrap(),
        format!(
            r#"[{{"literal":"g"}},{{"copy":{{"file":1,"lo":0,"hi":4}}}},{{"spelled":{{"key":"{}","text":"::n::g"}}}}]"#,
            d(0x10)
        )
    );
    let base = ["sections", "edits", "claims", "0", "pieces", "0"];
    for (new, path) in [
        (r#""g""#, "sections.edits.claims[0].pieces[0]"),
        ("{}", "sections.edits.claims[0].pieces[0]"),
        (
            r#"{"literal": "g", "copy": {"file": 0, "lo": 0, "hi": 1}}"#,
            "sections.edits.claims[0].pieces[0]",
        ),
        (r#"{"paste": "g"}"#, "sections.edits.claims[0].pieces[0].paste"),
        (r#"{"literal": 1}"#, "sections.edits.claims[0].pieces[0].literal"),
        (r#"{"copy": {"file": 0, "hi": 1}}"#, "sections.edits.claims[0].pieces[0].copy.lo"),
        (
            r#"{"spelled": {"text": "g", "key": "00"}}"#,
            "sections.edits.claims[0].pieces[0].spelled.key",
        ),
    ] {
        let mut v = map_shard(sample()).to_json();
        set(&mut v, &base, parse(new));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, path, "{new}");
    }
}

fn ident(text: &[u8], key: Option<Digest>) -> Piece {
    Piece::Identifier { text: text.to_vec(), key }
}

/// The sample with claim 1's piece an identifier piece, keyed or not.
fn with_identifier(key: Option<Digest>) -> Edits {
    let mut x = sample();
    x.claims[1].pieces = vec![ident(b"g", key)];
    x
}

// §4.2 (WP-132): an identifier piece, tag 3, `(bytes, opt<KeyId>)`, with
// and without its key: both round trips, and its JSON form, one member
// `identifier` with `key` only when present.
#[test]
fn identifier_piece_round_trip_and_json() {
    for key in [None, Some(d(0x10))] {
        round_trip(&map_shard(with_identifier(key)));
        let v = map_shard(with_identifier(key)).to_json();
        let piece = v
            .get("sections")
            .unwrap()
            .get("edits")
            .unwrap()
            .get("claims")
            .unwrap()
            .as_array()
            .unwrap()[1]
            .get("pieces")
            .unwrap()
            .clone();
        let want = match key {
            None => r#"[{"identifier":"g"}]"#.to_owned(),
            Some(k) => format!(r#"[{{"identifier":"g","key":"{k}"}}]"#),
        };
        assert_eq!(json::write_compact(&piece).unwrap(), want);
    }
    // Byte for byte: tag 3, the bytes, then the opt tag and the key.
    let s = map_shard(with_identifier(Some(d(0x10))));
    let bytes = s.to_bytes();
    let p = at(&s, "sections.edits.claims[1].pieces[0]");
    assert_eq!(&bytes[p..p + 4], [3, 1, b'g', 1]);
    assert_eq!(bytes[p + 4..p + 36], [0x10; 32]);
    let s = map_shard(with_identifier(None));
    let bytes = s.to_bytes();
    let p = at(&s, "sections.edits.claims[1].pieces[0]");
    assert_eq!(&bytes[p..p + 4], [3, 1, b'g', 0]);
    // JSON errors: a key member alone, in the wrong order, or not a key.
    let base = ["sections", "edits", "claims", "1", "pieces", "0"];
    for (new, path) in [
        (r#"{"key": "00"}"#, "sections.edits.claims[1].pieces[0].key"),
        (r#"{"key": "00", "identifier": "g"}"#, "sections.edits.claims[1].pieces[0]"),
        (r#"{"identifier": "g", "key": "00"}"#, "sections.edits.claims[1].pieces[0].key"),
        (r#"{"identifier": 1}"#, "sections.edits.claims[1].pieces[0].identifier"),
    ] {
        let mut v = map_shard(with_identifier(None)).to_json();
        set(&mut v, &base, parse(new));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, path, "{new}");
    }
}

// §4.2, §4.7 row 9 (WP-132): an identifier piece's bytes are one plain
// identifier (`spec/api.md` §6.1 rule 1), an invariant the reader checks:
// empty, a leading digit, a keyword, an alternative token, a qualified
// name, a byte outside the basic set are Malformed at the piece. With or
// without a key.
#[test]
fn identifier_piece_is_plain() {
    for key in [None, Some(d(0x10))] {
        for text in [&b""[..], b"1g", b"class", b"and", b"n::g", b"a b", b"g$", b"\xc3\xa9"] {
            let what = broken(
                |x| x.claims[1].pieces = vec![ident(text, key)],
                "sections.edits.claims[1].pieces[0]",
            );
            assert_eq!(what, "an identifier piece's text is not one plain identifier");
        }
        // The identifiers with special meaning are identifiers.
        for text in [&b"final"[..], b"override", b"import", b"module", b"_G9"] {
            let mut x = sample();
            x.claims[1].pieces = vec![ident(text, key)];
            map_shard(x).validate().unwrap();
        }
    }
}

// §4.2 (WP-132): a reader refuses a piece tag it does not list as
// Malformed at the tag, whatever follows. This is how a reader of before
// WP-132, which lists tags 0 to 2, refuses an identifier piece: the format
// versions stay, and an old reader fails at the first identifier piece
// rather than misreading it. Shown here with tag 4, the first unlisted.
#[test]
fn identifier_piece_unlisted_tag_is_refused() {
    for key in [None, Some(d(0x10))] {
        let s = map_shard(with_identifier(key));
        let bytes = s.to_bytes();
        let p = at(&s, "sections.edits.claims[1].pieces[0]");
        let e = Shard::from_bytes(&patch(&bytes, p, 3, 4)).expect_err("tag 4 is unlisted");
        assert_eq!((e.kind, e.offset, e.what), (ErrorKind::Malformed, p, "unlisted Piece value"));
    }
}

// spec/model.md §10, spec/landed/edits/WP-060.md §2: members named and
// ordered as the fields (`macro_site` is `macro`); enumerations by name.
#[test]
#[allow(clippy::many_single_char_names, reason = "one record each: e, c, s, r, v")]
fn edits_json_members() {
    let v = map_shard(sample()).to_json();
    let names = |v: &Value| -> Vec<String> {
        v.as_object().unwrap().iter().map(|(k, _)| k.clone()).collect()
    };
    let e = v.get("sections").unwrap().get("edits").unwrap();
    assert_eq!(
        names(e),
        [
            "scheme",
            "transform",
            "failure",
            "files",
            "subjects",
            "links",
            "claims",
            "groups",
            "reports",
            "changed",
            "renamed"
        ]
    );
    let c = &e.get("claims").unwrap().as_array().unwrap()[1];
    assert_eq!(
        names(c),
        [
            "file", "lo", "hi", "stance", "pieces", "subjects", "reason", "tiers", "macro",
            "votes", "notes"
        ]
    );
    assert_eq!(c.get("stance").unwrap().as_str(), Some("replace"));
    let s = &e.get("subjects").unwrap().as_array().unwrap()[1];
    assert_eq!(names(s), ["id", "rule", "role", "of", "flags", "display"]);
    assert_eq!(s.get("role").unwrap().as_str(), Some("follower"));
    let r = &e.get("reports").unwrap().as_array().unwrap()[1];
    assert_eq!(names(r), ["place", "severity", "message", "attached"]);
    assert_eq!(r.get("attached"), Some(&Value::Bool(true)));
    let sections = names(v.get("sections").unwrap());
    assert_eq!(sections, ["header", "outcome", "inputs", "counters", "edits"]);
    // Wrong names and types are errors at their paths.
    for (path, new, at) in [
        (
            &["sections", "edits", "claims", "3", "reason"][..],
            "\"MacroBody\"",
            "sections.edits.claims[3].reason",
        ),
        (
            &["sections", "edits", "claims", "0", "stance"][..],
            "\"edit\"",
            "sections.edits.claims[0].stance",
        ),
        (
            &["sections", "edits", "subjects", "0", "role"][..],
            "0",
            "sections.edits.subjects[0].role",
        ),
        (
            &["sections", "edits", "reports", "0", "severity"][..],
            "\"fatal\"",
            "sections.edits.reports[0].severity",
        ),
        (
            &["sections", "edits", "reports", "0", "attached"][..],
            "0",
            "sections.edits.reports[0].attached",
        ),
        (&["sections", "edits", "groups", "0"][..], "{}", "sections.edits.groups[0]"),
        (&["sections", "edits", "groups", "0"][..], "[0, -1]", "sections.edits.groups[0][1]"),
        (
            &["sections", "edits", "renamed", "0", "name"][..],
            "{\"operator\": 0}",
            "sections.edits.renamed[0].name.operator",
        ),
        (
            &["sections", "edits", "subjects", "0", "rule", "kind"][..],
            "\"fn\"",
            "sections.edits.subjects[0].rule.kind",
        ),
        (
            &["sections", "edits", "claims", "1", "macro"][..],
            "{\"name\": \"ID\"}",
            "sections.edits.claims[1].macro.name_start",
        ),
    ] {
        let mut v = map_shard(sample()).to_json();
        set(&mut v, path, parse(new));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, at, "{at}");
    }
}

// spec/model.md §10: the JSON paths in a failure: its kind, its place, its
// refusal's subject.
#[test]
fn failure_json_paths() {
    let mut v = map_shard(failed_run()).to_json();
    set(&mut v, &["sections", "edits", "failure", "kind"], parse("\"panic\""));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.edits.failure.kind");
    for (new, at) in [
        ("5", "sections.edits.failure.place"),
        (r#"{"file": "/t/x.cc", "line": -3, "column": 7}"#, "sections.edits.failure.place.line"),
        (r#"{"file": 0, "line": 3, "column": 7}"#, "sections.edits.failure.place.file"),
    ] {
        let mut v = map_shard(failed_run()).to_json();
        set(&mut v, &["sections", "edits", "failure", "place"], parse(new));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, at, "{new}");
    }
    let mut v = map_shard(failed_run()).to_json();
    set(&mut v, &["sections", "edits", "failure", "refusal", "subject", "lo"], parse("\"x\""));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.edits.failure.refusal.subject.lo");
}

// spec/model.md §1 rule 6, §2: unlisted values of the section's
// enumerations, the piece tag and opt tags are Malformed at their first
// byte; truncation and leftover bytes as for any section.
#[test]
fn edits_unlisted_values() {
    let bytes = small_bytes();
    for (at, from, to) in [
        (97, 0, 2),   // failure opt tag
        (141, 0, 4),  // stance
        (143, 0, 4),  // piece tag (3 is an identifier piece, WP-132)
        (187, 0, 11), // reason
        (189, 0, 2),  // macro opt tag
        (196, 0, 2),  // report place opt tag
        (197, 0, 3),  // severity
        (200, 0, 2),  // attached
    ] {
        assert_eq!(
            error(Shard::from_bytes(&patch(&bytes, at, from, to))),
            (ErrorKind::Malformed, at),
            "{at}"
        );
    }
    // Truncated inside the section; a byte left over after it.
    let s = small();
    let mut payload = raw_edits(s.edits.as_ref().unwrap()).e.into_bytes();
    let with = |p: &[u8]| {
        let mut w = Writer::new(*b"SHRD", 1);
        let full = raw(&s);
        for id in [1u64, 2, 4] {
            let at = payload_at(&full, id);
            let len = match id {
                1 => 34,
                2 => 4,
                _ => 1,
            };
            w.section(id, &full[at..at + len]);
        }
        w.section(17, p);
        w.finish()
    };
    assert_eq!(with(&payload), small_bytes());
    // spec/model.md §2 (tagged union), §2.2: a piece's tag is a `uv`; a
    // non-minimal one (80 00 for 0) is Malformed at its redundant byte, an
    // unlisted one at its first byte, also when it takes two bytes.
    let tag = 143 - 64;
    assert_eq!(payload[tag], 0);
    let mut long = payload.clone();
    long.insert(tag, 0x80);
    assert_eq!(error(Shard::from_bytes(&with(&long))), (ErrorKind::Malformed, 144));
    let mut wide = payload.clone();
    wide.splice(tag..=tag, [0x83, 0x01]);
    assert_eq!(error(Shard::from_bytes(&with(&wide))), (ErrorKind::Malformed, 143));
    let short = &payload[..payload.len() - 1];
    assert_eq!(error(Shard::from_bytes(&with(short))).0, ErrorKind::Truncated);
    payload.push(0);
    assert_eq!(error(Shard::from_bytes(&with(&payload))), (ErrorKind::Malformed, 203));
    // A failure kind and a subject role out of their tables.
    let mut s = map_shard(failed_run());
    let bytes = s.to_bytes();
    let base = payload_at(&bytes, 17);
    assert_eq!(
        error(Shard::from_bytes(&patch(&bytes, base + 34, 0, 6))),
        (ErrorKind::Malformed, base + 34)
    );
    s.edits = Some(sample());
    let bytes = s.to_bytes();
    let role = at(&s, "sections.edits.subjects[1].of") - 1;
    assert_eq!(error(Shard::from_bytes(&patch(&bytes, role, 2, 3))), (ErrorKind::Malformed, role));
}

// spec/edits.md §4.2: every value of `Reason` is written as its number
// and read back, in binary and in JSON (by its name); the number after
// the last, 11, is Malformed (`edits_unlisted_values`). `directive` (10)
// was added by WP-068.
#[test]
fn edits_reason_values() {
    let all = [
        (Reason::MacroBody, 1, "macro_body"),
        (Reason::Pasted, 2, "pasted"),
        (Reason::Crossing, 3, "crossing"),
        (Reason::NotAFile, 4, "not_a_file"),
        (Reason::NotOwned, 5, "not_owned"),
        (Reason::FileRecoded, 6, "file_recoded"),
        (Reason::NotPlain, 7, "not_plain"),
        (Reason::MixedSet, 8, "mixed_set"),
        (Reason::MixedInstances, 9, "mixed_instances"),
        (Reason::Directive, 10, "directive"),
    ];
    for (reason, n, name) in all {
        assert_eq!(reason as u64, n, "{name}");
        let s = map_shard(Edits {
            files: vec![file(b"/s/a.cc", 100, 1)],
            claims: vec![Claim { reason, ..claim(0, 1, 2, Stance::Refuse) }],
            ..empty_edits()
        });
        round_trip(&s);
        let v = s.to_json();
        let c = &v.get("sections").unwrap().get("edits").unwrap().get("claims").unwrap();
        let got = c.as_array().unwrap()[0].get("reason").unwrap();
        assert_eq!(got, &Value::Str(name.to_owned()), "{name}");
    }
}

// spec/model.md §5 rule 2, §8: section 17 is known; a reader of this crate
// reads it.
#[test]
fn edits_section_is_known() {
    let bytes = small_bytes();
    assert_eq!(payload_at(&bytes, 17), 64);
    assert!(Shard::from_bytes(&bytes).unwrap().edits.is_some());
}

// ---------------------------------------------------------------------------
// Invariants (spec/edits.md §4.7, spec/model.md §8)

/// The offset at which `from_bytes` rejects `s`'s file, after checking
/// that it is the offset of `path` and that `validate`, `from_json` and
/// `undump` reject it at `path` with the same message.
fn rejected(s: &Shard, path: &str) -> String {
    let e = Shard::from_bytes(&raw(s)).expect_err("from_bytes accepted an invalid shard");
    assert_eq!(e.kind, ErrorKind::Malformed, "{e}");
    assert_eq!(e.offset, at(s, path), "offset of {path}: {}", e.what);
    let v = s.validate().expect_err("validate accepted an invalid shard");
    assert_eq!(v, format!("{path}: {}", e.what), "validate");
    let j = Shard::from_json(&s.to_json()).expect_err("from_json accepted an invalid shard");
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: e.what.to_owned() });
    assert_eq!(undump(&s.to_json()).unwrap_err(), j);
    e.what.to_owned()
}

/// `rejected` for the sample changed by `f`.
fn broken(f: impl FnOnce(&mut Edits), path: &str) -> String {
    let mut x = sample();
    f(&mut x);
    rejected(&map_shard(x), path)
}

// spec/model.md §8: an edits section only in a map shard whose status is
// ok.
#[test]
fn edits_phase_and_status() {
    let mut s = map_shard(sample());
    s.phase = Phase::Verify;
    assert!(rejected(&s, "sections.edits").contains("phase is not map"));
    for status in [Status::Diagnosed, Status::Rejected] {
        let mut s = map_shard(sample());
        s.outcome.status = status;
        assert!(rejected(&s, "sections.edits").contains("status is not ok"));
    }
    let mut s = map_shard(sample());
    s.outcome.status = Status::Failed;
    s.inputs = None;
    s.counters.clear();
    assert!(rejected(&s, "sections.edits").contains("status is not ok"));
    // Phase before status.
    s.phase = Phase::Verify;
    assert!(rejected(&s, "sections.edits").contains("phase is not map"));
}

// §4.7 row 1: scheme is 1.
#[test]
fn edits_scheme() {
    for scheme in [0, 2] {
        broken(|x| x.scheme = scheme, "sections.edits.scheme");
    }
}

// §4.7 row 2: with a failure, the seven lists are empty and files has at
// most one entry; the first list that is not, in field order.
#[test]
fn edits_failure_lists() {
    let failed = |f: fn(&mut Edits), path: &str| {
        let mut x = failed_run();
        f(&mut x);
        rejected(&map_shard(x), path)
    };
    failed(|x| x.files.push(file(b"/s/b.cc", 1, 9)), "sections.edits.files");
    failed(|x| x.subjects = sample().subjects, "sections.edits.subjects");
    failed(|x| x.links = sample().links, "sections.edits.links");
    failed(|x| x.claims.push(claim(0, 30, 31, Stance::Keep)), "sections.edits.claims");
    failed(|x| x.groups.push(vec![0, 1]), "sections.edits.groups");
    failed(|x| x.reports = sample().reports[..1].to_vec(), "sections.edits.reports");
    failed(|x| x.changed = sample().changed, "sections.edits.changed");
    failed(|x| x.renamed = sample().renamed, "sections.edits.renamed");
    // Two files and a claim: the files are reported.
    failed(
        |x| {
            x.files.push(file(b"/s/b.cc", 1, 9));
            x.claims.push(claim(0, 30, 31, Stance::Keep));
        },
        "sections.edits.files",
    );
    // A subject and a rule: the subjects.
    failed(
        |x| {
            x.renamed = sample().renamed;
            x.subjects = sample().subjects;
        },
        "sections.edits.subjects",
    );
    // A link and a claim: the links (field order).
    failed(
        |x| {
            x.claims.push(claim(0, 30, 31, Stance::Keep));
            x.links = sample().links;
        },
        "sections.edits.links",
    );
    // A change and a rule: the changes.
    failed(
        |x| {
            x.renamed = sample().renamed;
            x.changed = sample().changed;
        },
        "sections.edits.changed",
    );
}

// §4.7, the refusal row: a failed run's `refusal.subject` is within its
// file, and `files` is empty unless the refusal has a subject.
#[test]
fn edits_failure_refusal_subject() {
    const AT: &str = "sections.edits.failure.refusal.subject";
    let failed = |f: &dyn Fn(&mut Edits), path: &str| {
        let mut x = failed_run();
        f(&mut x);
        rejected(&map_shard(x), path)
    };
    let subject = |x: &mut Edits, p: Option<Place>| {
        x.failure.as_mut().unwrap().refusal.as_mut().unwrap().subject = p;
    };
    for p in
        [place(7, 5, 1), place(1, 0, 1), place(0, 5, 1), place(0, 0, 101), place(u64::MAX, 0, 0)]
    {
        let what = failed(&|x| subject(x, Some(p)), AT);
        assert_eq!(what, "the refusal's subject is not within its file");
    }
    // A subject and no file.
    failed(&|x| x.files.clear(), AT);
    // A file and no subject; a file and no refusal.
    let what = failed(&|x| subject(x, None), "sections.edits.files");
    assert_eq!(what, "a failed run names a file but its refusal has no subject");
    failed(&|x| x.failure.as_mut().unwrap().refusal = None, "sections.edits.files");
    // Valid: an empty subject at the end of the file; no file, no subject.
    let mut x = failed_run();
    subject(&mut x, Some(place(0, 100, 100)));
    map_shard(x).validate().unwrap();
    let mut x = failed_run();
    subject(&mut x, None);
    x.files.clear();
    round_trip(&map_shard(x));
    // After the lists, and after the count of files.
    failed(
        &|x| {
            subject(x, Some(place(7, 0, 0)));
            x.claims.push(claim(0, 30, 31, Stance::Keep));
        },
        "sections.edits.claims",
    );
    failed(
        &|x| {
            subject(x, Some(place(7, 0, 0)));
            x.files.push(file(b"/s/b.cc", 1, 9));
        },
        "sections.edits.files",
    );
}

// §4.7 row 3: files strictly ascending by path.
#[test]
fn edits_files_ascending() {
    broken(|x| x.files[1].path = x.files[0].path.clone(), "sections.edits.files[1]");
    broken(|x| x.files.swap(1, 2), "sections.edits.files[2]");
}

// §4.7 rows 4-6: subjects ascending by id; of iff follower, ascending;
// flag bits above 3.
#[test]
fn edits_subjects() {
    broken(|x| x.subjects[2].id = d(0x11), "sections.edits.subjects[2]");
    broken(|x| x.subjects.swap(0, 1), "sections.edits.subjects[1]");
    broken(|x| x.subjects[1].of.clear(), "sections.edits.subjects[1].of");
    broken(|x| x.subjects[0].of.push(d(0x12)), "sections.edits.subjects[0].of");
    broken(|x| x.subjects[1].of = vec![d(0x12), d(0x10)], "sections.edits.subjects[1].of");
    broken(|x| x.subjects[1].of = vec![d(0x10), d(0x10)], "sections.edits.subjects[1].of");
    for flags in [16, 1 << 63] {
        broken(|x| x.subjects[2].flags = flags, "sections.edits.subjects[2].flags");
    }
    // Each rule over the whole list before the next: subject 2's of is
    // reported before subject 0's flags.
    broken(
        |x| {
            x.subjects[0].flags = 16;
            x.subjects[2].of.push(d(0x10));
        },
        "sections.edits.subjects[2].of",
    );
}

// §4.7 row 7: links strictly ascending; a < b.
#[test]
fn edits_links() {
    broken(|x| x.links[0] = Link { a: d(0x12), b: d(0x10) }, "sections.edits.links[0]");
    broken(|x| x.links[0].b = d(0x10), "sections.edits.links[0]");
    broken(|x| x.links.push(x.links[0]), "sections.edits.links[1]");
    broken(|x| x.links.insert(0, Link { a: d(0x11), b: d(0x12) }), "sections.edits.links[1]");
}

// §4.7 row 8: a claim's file is an index of files; lo <= hi <= size;
// lo < hi for replace.
#[test]
fn edits_claim_range() {
    broken(|x| x.claims[5].file = 3, "sections.edits.claims[5]");
    broken(|x| x.claims[5].file = u64::MAX, "sections.edits.claims[5]");
    broken(|x| x.claims[4].hi = 41, "sections.edits.claims[4]");
    broken(|x| x.claims[4].lo = 3, "sections.edits.claims[4]");
    broken(|x| x.claims[0].hi = 10, "sections.edits.claims[0]");
    // An empty range is allowed for the other stances; the size is allowed.
    let mut x = sample();
    x.claims[4].lo = 2;
    x.claims[5].lo = 40;
    x.claims[5].hi = 40;
    map_shard(x).validate().unwrap();
}

// §4.7 row 9: pieces empty unless replace; no empty literal or spelled
// text; no two literals adjacent; a copy's place within its file and not
// empty.
#[test]
fn edits_claim_pieces() {
    broken(|x| x.claims[2].pieces.push(literal(b"k")), "sections.edits.claims[2].pieces");
    broken(|x| x.claims[0].pieces[0] = literal(b""), "sections.edits.claims[0].pieces[0]");
    broken(
        |x| x.claims[0].pieces[2] = Piece::Spelled { key: d(0x10), text: Vec::new() },
        "sections.edits.claims[0].pieces[2]",
    );
    broken(|x| x.claims[0].pieces.insert(1, literal(b"h")), "sections.edits.claims[0].pieces[1]");
    broken(
        |x| x.claims[0].pieces[1] = Piece::Copy(place(1, 4, 4)),
        "sections.edits.claims[0].pieces[1]",
    );
    broken(
        |x| x.claims[0].pieces[1] = Piece::Copy(place(1, 0, 61)),
        "sections.edits.claims[0].pieces[1]",
    );
    broken(
        |x| x.claims[0].pieces[1] = Piece::Copy(place(3, 0, 1)),
        "sections.edits.claims[0].pieces[1]",
    );
    // A replace with no piece deletes; a literal after a copy is fine.
    let mut x = sample();
    x.claims[1].pieces.clear();
    x.claims[0].pieces.push(literal(b"z"));
    map_shard(x).validate().unwrap();
}

// §4.7 row 10: reason is none iff the stance is not refuse.
#[test]
fn edits_claim_reason() {
    broken(|x| x.claims[3].reason = Reason::None, "sections.edits.claims[3].reason");
    broken(|x| x.claims[0].reason = Reason::NotPlain, "sections.edits.claims[0].reason");
    broken(|x| x.claims[5].reason = Reason::Crossing, "sections.edits.claims[5].reason");
}

// §4.7 row 11: subjects and notes strictly ascending.
#[test]
fn edits_claim_subjects_and_notes() {
    broken(|x| x.claims[1].subjects.reverse(), "sections.edits.claims[1].subjects");
    broken(|x| x.claims[0].subjects.push(d(0x10)), "sections.edits.claims[0].subjects");
    broken(|x| x.claims[0].notes.reverse(), "sections.edits.claims[0].notes");
    broken(
        |x| x.claims[2].notes = vec![b"a".to_vec(), b"a".to_vec()],
        "sections.edits.claims[2].notes",
    );
    // Subjects before notes.
    broken(
        |x| {
            x.claims[0].notes.reverse();
            x.claims[1].subjects.reverse();
            x.claims[0].subjects.push(d(0x10));
        },
        "sections.edits.claims[0].subjects",
    );
}

// §4.7 row 12: tiers not 0, no bit above 4, bits 2 to 4 only for refuse;
// macro absent if tiers is bit 0 alone.
#[test]
fn edits_claim_tiers() {
    broken(|x| x.claims[0].tiers = 0, "sections.edits.claims[0].tiers");
    broken(|x| x.claims[3].tiers = 32, "sections.edits.claims[3].tiers");
    for t in [tiers::T2, tiers::T3, tiers::X, tiers::T0 | tiers::X] {
        broken(|x| x.claims[1].tiers = t, "sections.edits.claims[1].tiers");
        broken(|x| x.claims[2].tiers = t, "sections.edits.claims[2].tiers");
        broken(|x| x.claims[5].tiers = t, "sections.edits.claims[5].tiers");
    }
    broken(|x| x.claims[0].macro_site = macro_site(1, b"M"), "sections.edits.claims[0].tiers");
    // The bits by value, not by the crate's constants: 31 is every tier,
    // 16 tier X alone.
    for t in [31, 16, 4 | 16] {
        let mut x = sample();
        x.claims[3].tiers = t;
        map_shard(x).validate().unwrap();
    }
    for t in [32, 33, 1 << 63] {
        let what = broken(|x| x.claims[3].tiers = t, "sections.edits.claims[3].tiers");
        assert_eq!(what, "tiers has a bit above 4 set");
    }
    assert_eq!(
        broken(|x| x.claims[2].tiers = 16, "sections.edits.claims[2].tiers"),
        "tiers has a bit of tier 2, 3 or X for a claim that is not a refuse"
    );
    assert_eq!(broken(|x| x.claims[0].tiers = 0, "sections.edits.claims[0].tiers"), "tiers is 0");
    // Every tier for a refuse; a macro with tier 1.
    let mut x = sample();
    x.claims[3].tiers = tiers::ALL;
    x.claims[0].tiers = tiers::T1;
    x.claims[0].macro_site = macro_site(1, b"M");
    map_shard(x).validate().unwrap();
}

// §4.7 row 13: votes at least 1.
#[test]
fn edits_claim_votes() {
    broken(|x| x.claims[4].votes = 0, "sections.edits.claims[4].votes");
}

// §4.7 row 14: claims strictly ascending in the sort key (§4.2).
#[test]
fn edits_claims_ascending() {
    broken(|x| x.claims.swap(1, 2), "sections.edits.claims[2]");
    // Equal in the key, different in what is not part of it.
    broken(
        |x| {
            let mut c = x.claims[2].clone();
            c.votes = 3;
            x.claims.insert(3, c);
        },
        "sections.edits.claims[3]",
    );
    // The key's pieces: by tag, then fields.
    broken(
        |x| {
            let mut c = x.claims[0].clone();
            c.pieces = vec![literal(b"a")];
            x.claims.insert(1, c);
        },
        "sections.edits.claims[1]",
    );
    broken(
        |x| {
            let mut c = x.claims[0].clone();
            c.pieces = vec![literal(b"g"), Piece::Copy(place(0, 0, 1))];
            x.claims.insert(1, c);
        },
        "sections.edits.claims[1]",
    );
    // A proper prefix of the pieces first: [g] then [g, copy] is ascending.
    let mut x = sample();
    let mut c = x.claims[1].clone();
    c.pieces.push(Piece::Copy(place(0, 0, 1)));
    x.claims.insert(2, c);
    x.groups.clear();
    map_shard(x).validate().unwrap();
}

// §4.7: the claim rules are checked for each claim in turn: claim 0's
// votes before claim 1's range, though the range rule comes first.
#[test]
fn edits_claim_rules_per_claim() {
    broken(
        |x| {
            x.claims[0].votes = 0;
            x.claims[1].hi = 101;
        },
        "sections.edits.claims[0].votes",
    );
    // The claim rules after the link rules, before the group rules.
    broken(
        |x| {
            x.links[0].b = d(0x10);
            x.claims[0].votes = 0;
        },
        "sections.edits.links[0]",
    );
    broken(
        |x| {
            x.groups[0] = vec![1];
            x.claims[5].votes = 0;
        },
        "sections.edits.claims[5].votes",
    );
}

// §4.7 row 15: groups.
#[test]
fn edits_groups() {
    broken(|x| x.groups[0] = vec![0], "sections.edits.groups[0]");
    broken(|x| x.groups[0] = vec![1, 0], "sections.edits.groups[0]");
    broken(|x| x.groups[0] = vec![0, 0], "sections.edits.groups[0]");
    broken(|x| x.groups[0] = vec![0, 2], "sections.edits.groups[0]");
    broken(|x| x.groups[0] = vec![0, 6], "sections.edits.groups[0]");
    let two_replaces = |x: &mut Edits| {
        let mut c = x.claims[1].clone();
        c.lo = 25;
        c.hi = 26;
        x.claims.insert(2, c);
        let mut c = x.claims[1].clone();
        c.lo = 27;
        c.hi = 28;
        x.claims.insert(3, c);
    };
    broken(
        |x| {
            two_replaces(x);
            x.groups.push(vec![1, 2]);
        },
        "sections.edits.groups[1]",
    );
    broken(
        |x| {
            two_replaces(x);
            x.groups = vec![vec![2, 3], vec![0, 1]];
        },
        "sections.edits.groups[1]",
    );
    let mut x = sample();
    two_replaces(&mut x);
    x.groups = vec![vec![0, 1], vec![2, 3]];
    map_shard(x).validate().unwrap();
}

// §4.7 row 16: reports strictly ascending; a place within its file.
#[test]
fn edits_reports() {
    broken(|x| x.reports.swap(0, 1), "sections.edits.reports[1]");
    broken(|x| x.reports.swap(1, 2), "sections.edits.reports[2]");
    broken(|x| x.reports.push(x.reports[2].clone()), "sections.edits.reports[3]");
    broken(|x| x.reports[2].place = Some(place(0, 10, 101)), "sections.edits.reports[2]");
    broken(|x| x.reports[1].place = Some(place(3, 0, 0)), "sections.edits.reports[1]");
    // attached orders false before true.
    broken(
        |x| {
            let mut r = x.reports[0].clone();
            r.attached = true;
            x.reports[0] = r;
            x.reports.insert(1, Report { attached: false, ..x.reports[0].clone() });
        },
        "sections.edits.reports[1]",
    );
    // An empty place is allowed for a report.
    let mut x = sample();
    x.reports[1].place = Some(place(2, 40, 40));
    x.reports.swap(1, 2);
    x.reports.sort();
    map_shard(x).validate().unwrap();
}

// §4.7 row 17: changed strictly ascending; a place within its file and not
// empty.
#[test]
fn edits_changed() {
    broken(|x| x.changed[0].place.hi = 10, "sections.edits.changed[0]");
    broken(|x| x.changed[0].place.file = 3, "sections.edits.changed[0]");
    broken(|x| x.changed[0].place.hi = 101, "sections.edits.changed[0]");
    broken(|x| x.changed.push(x.changed[0].clone()), "sections.edits.changed[1]");
    broken(
        |x| x.changed.insert(0, Changed { key: d(0x21), ..x.changed[0].clone() }),
        "sections.edits.changed[1]",
    );
}

// §4.7 row 18: renamed strictly ascending by the encoded (parent, kind,
// name, disc); an invalid name is refused as a reader refuses it.
#[test]
fn edits_renamed() {
    broken(|x| x.renamed.swap(0, 1), "sections.edits.renamed[1]");
    let what = broken(
        |x| x.renamed.push(rule(Parent::Unit(d(5)), b"h", b"j")),
        "sections.edits.renamed[2]",
    );
    assert_eq!(what, "two rules with one (parent, kind, name, disc)");
    // Ordered by the match's bytes, not by the derived order of `Rule`:
    // Function (5) before Variable (6) whatever the names.
    let mut x = sample();
    x.renamed = vec![
        rule(Parent::Root, b"z", b"y"),
        Rule { kind: KeyKind::Variable, ..rule(Parent::Root, b"a", b"b") },
    ];
    map_shard(x).validate().unwrap();
    // A name no reader accepts, in memory: refused by validate.
    let mut x = sample();
    x.renamed[0].to_name = Name::Operator(0);
    assert_eq!(
        map_shard(x).validate().unwrap_err(),
        "sections.edits.renamed[0]: operator number is 0"
    );
    let mut x = sample();
    x.subjects[0].rule.as_mut().unwrap().name = Name::Operator(0);
    assert_eq!(
        map_shard(x).validate().unwrap_err(),
        "sections.edits.subjects[0].rule: operator number is 0"
    );
}

// §4.7: the order of two broken invariants is the table's.
#[test]
fn edits_order_of_rules() {
    broken(
        |x| {
            x.scheme = 2;
            x.files.swap(0, 1);
        },
        "sections.edits.scheme",
    );
    broken(
        |x| {
            x.files.swap(0, 1);
            x.subjects.swap(0, 1);
        },
        "sections.edits.files[1]",
    );
    broken(
        |x| {
            x.renamed.swap(0, 1);
            x.changed[0].place.hi = 10;
            x.reports.swap(0, 1);
        },
        "sections.edits.reports[1]",
    );
    broken(
        |x| {
            x.renamed.swap(0, 1);
            x.changed[0].place.hi = 10;
        },
        "sections.edits.changed[0]",
    );
    // The phase before every rule of the section.
    let mut x = sample();
    x.scheme = 0;
    let mut s = map_shard(x);
    s.phase = Phase::Verify;
    rejected(&s, "sections.edits");
}

// ---------------------------------------------------------------------------
// normalise (spec/edits.md §4.3, §4.4)

fn normalised(mut x: Edits) -> Edits {
    x.normalise();
    map_shard(x.clone()).validate().unwrap();
    x
}

/// A section with one file and the claims given.
fn with_claims(claims: Vec<Claim>, groups: Vec<Vec<u64>>) -> Edits {
    Edits { files: vec![file(b"/s/a.cc", 100, 1)], claims, groups, ..empty_edits() }
}

fn replace(lo: u64, hi: u64, text: &[u8]) -> Claim {
    Claim { pieces: vec![literal(text)], ..claim(0, lo, hi, Stance::Replace) }
}

// §4.3: derivations that agree in the sort key are one claim: votes
// summed, tiers or-ed, the least macro, the least reason other than none,
// subjects and notes united.
#[test]
#[allow(clippy::many_single_char_names, reason = "derivations a, b, c and their makers")]
fn normalise_joins_agreeing_derivations() {
    let a = Claim {
        subjects: vec![d(2)],
        notes: vec![b"b".to_vec()],
        tiers: tiers::T1,
        macro_site: macro_site(7, b"B"),
        votes: 2,
        ..replace(10, 11, b"g")
    };
    let b = Claim {
        subjects: vec![d(1), d(2)],
        notes: vec![b"a".to_vec(), b"b".to_vec()],
        tiers: tiers::T0,
        macro_site: macro_site(7, b"A"),
        votes: 3,
        ..replace(10, 11, b"g")
    };
    let c = Claim { macro_site: None, tiers: tiers::T0, ..replace(10, 11, b"g") };
    let x = normalised(with_claims(vec![a, b, c], Vec::new()));
    assert_eq!(x.claims.len(), 1);
    let j = &x.claims[0];
    assert_eq!(j.votes, 6);
    assert_eq!(j.tiers, tiers::T0 | tiers::T1);
    assert_eq!(j.macro_site, macro_site(7, b"A"));
    assert_eq!(j.subjects, [d(1), d(2)]);
    assert_eq!(j.notes, [b"a".to_vec(), b"b".to_vec()]);
    // The least macro by name_start first, then name.
    let m = |start: u64, name: &[u8]| Claim {
        tiers: tiers::T1,
        macro_site: macro_site(start, name),
        ..replace(10, 11, b"g")
    };
    let x = normalised(with_claims(vec![m(9, b"A"), m(3, b"Z"), m(3, b"Y")], Vec::new()));
    assert_eq!(x.claims[0].macro_site, macro_site(3, b"Y"));
    // ... whatever its place among the derivations: the least first.
    let x = normalised(with_claims(vec![m(3, b"Y"), m(9, b"A"), m(3, b"Z")], Vec::new()));
    assert_eq!(x.claims[0].macro_site, macro_site(3, b"Y"));
    // The least reason other than none.
    let r =
        |reason: Reason| Claim { reason, subjects: vec![d(1)], ..claim(0, 1, 2, Stance::Refuse) };
    let mut x =
        with_claims(vec![r(Reason::NotPlain), r(Reason::Pasted), r(Reason::NotOwned)], Vec::new());
    x.normalise();
    assert_eq!((x.claims.len(), x.claims[0].reason), (1, Reason::Pasted));
    let mut x = with_claims(vec![r(Reason::None), r(Reason::NotPlain)], Vec::new());
    x.normalise();
    assert_eq!(x.claims[0].reason, Reason::NotPlain);
    // Votes saturate.
    let v = Claim { votes: u64::MAX, ..replace(10, 11, b"g") };
    let x = normalised(with_claims(vec![v.clone(), v], Vec::new()));
    assert_eq!(x.claims[0].votes, u64::MAX);
}

// §4.3: derivations that differ in a field of the key are several claims,
// sorted by the key.
#[test]
fn normalise_keeps_derivations_that_differ() {
    let base = replace(10, 11, b"g");
    let spelled = |key: u8| Claim {
        pieces: vec![Piece::Spelled { key: d(key), text: b"g".to_vec() }],
        ..base.clone()
    };
    let other_file = Claim { file: 1, ..base.clone() };
    for other in [
        other_file,
        Claim { lo: 9, ..base.clone() },
        Claim { hi: 12, ..base.clone() },
        Claim { stance: Stance::Keep, pieces: Vec::new(), ..base.clone() },
        replace(10, 11, b"h"),
        Claim { pieces: vec![literal(b"g"), Piece::Copy(place(0, 0, 1))], ..base.clone() },
        Claim { pieces: vec![Piece::Copy(place(0, 0, 1))], ..base.clone() },
        spelled(1),
    ] {
        let mut x = with_claims(vec![other.clone(), base.clone()], Vec::new());
        x.files.push(file(b"/s/b.cc", 100, 2));
        let x = normalised(x);
        assert_eq!(x.claims.len(), 2, "{other:?}");
        assert!(x.claims.contains(&other) && x.claims.contains(&base));
        assert!(x.claims[0].cmp_key(&x.claims[1]).is_lt());
    }
    // A spelled piece's key is part of the key.
    let x = normalised(with_claims(vec![spelled(2), spelled(1), spelled(2)], Vec::new()));
    assert_eq!(x.claims, [spelled(1), Claim { votes: 2, ..spelled(2) }]);
}

// §4.4: groups follow their members through the sort, groups that share a
// member are joined, and a group of one is dropped.
#[test]
fn normalise_groups() {
    // Members moved by the sort: [30) is claim 2 and [10) claim 0 after it.
    let x = normalised(with_claims(
        vec![replace(30, 31, b"c"), replace(20, 21, b"b"), replace(10, 11, b"a")],
        vec![vec![0, 2]],
    ));
    assert_eq!(x.claims[0], replace(10, 11, b"a"));
    assert_eq!(x.groups, [vec![0, 2]]);
    let x = normalised(with_claims(
        vec![replace(30, 31, b"c"), replace(20, 21, b"b"), replace(10, 11, b"a")],
        vec![vec![0, 1]],
    ));
    assert_eq!(x.groups, [vec![1, 2]]);
    // Two groups that share a member are one; others stay apart, sorted.
    let claims: Vec<Claim> = (0..6).map(|i| replace(10 * i + 1, 10 * i + 2, b"x")).collect();
    let x = normalised(with_claims(
        claims.clone(),
        vec![vec![4, 5], vec![2, 3], vec![0, 1], vec![1, 2]],
    ));
    assert_eq!(x.groups, [vec![0, 1, 2, 3], vec![4, 5]]);
    // A group reduced to one member by the join of two claims.
    let x = normalised(with_claims(
        vec![replace(10, 11, b"a"), replace(10, 11, b"a"), replace(20, 21, b"b")],
        vec![vec![0, 1]],
    ));
    assert_eq!(x.claims.len(), 2);
    assert_eq!(x.claims[0].votes, 2);
    assert!(x.groups.is_empty());
    // ... but a join that leaves two members keeps the group.
    let x = normalised(with_claims(
        vec![replace(10, 11, b"a"), replace(10, 11, b"a"), replace(20, 21, b"b")],
        vec![vec![0, 1, 2]],
    ));
    assert_eq!(x.groups, [vec![0, 1]]);
    // Unsorted members and a group given twice.
    let x = normalised(with_claims(claims, vec![vec![3, 1], vec![1, 3]]));
    assert_eq!(x.groups, [vec![1, 3]]);
}

// normalise: files sorted with every index following them; subjects,
// links, reports, changed and rules sorted and made unique.
#[test]
fn normalise_other_lists() {
    let mut x = sample();
    x.files.swap(0, 2);
    let remap = |f: &mut u64| *f = [2, 1, 0][*f as usize];
    for c in &mut x.claims {
        remap(&mut c.file);
        for p in &mut c.pieces {
            if let Piece::Copy(p) = p {
                remap(&mut p.file);
            }
        }
    }
    for r in &mut x.reports {
        if let Some(p) = &mut r.place {
            remap(&mut p.file);
        }
    }
    for c in &mut x.changed {
        remap(&mut c.place.file);
    }
    x.claims.reverse();
    x.groups = vec![vec![5, 4]];
    x.subjects.reverse();
    x.subjects[1].of = vec![d(0x10), d(0x10)];
    x.subjects.push(x.subjects[1].clone());
    x.links = vec![Link { a: d(0x12), b: d(0x10) }, Link { a: d(0x10), b: d(0x12) }];
    x.reports.reverse();
    x.reports.push(x.reports[0].clone());
    x.changed.push(x.changed[0].clone());
    x.renamed.reverse();
    x.renamed.push(x.renamed[0].clone());
    x.files.push(x.files[1].clone());
    assert_eq!(normalised(x), sample());
    // The refusal's subject follows its file.
    let mut x = failed_run();
    x.files.insert(0, file(b"/s/b.cc", 9, 9));
    x.failure.as_mut().unwrap().refusal.as_mut().unwrap().subject = Some(place(1, 1, 2));
    x.normalise();
    assert_eq!(x.files[0].path, b"/s/a.cc");
    assert_eq!(x.failure.unwrap().refusal.unwrap().subject, Some(place(0, 1, 2)));
}

// §4.2: the order of pieces and claims.
#[test]
fn piece_and_claim_order() {
    let ps = [
        literal(b""),
        literal(b"a"),
        literal(b"ab"),
        literal(b"b"),
        Piece::Copy(place(0, 5, 9)),
        Piece::Copy(place(0, 6, 7)),
        Piece::Copy(place(1, 0, 1)),
        Piece::Spelled { key: d(1), text: b"z".to_vec() },
        Piece::Spelled { key: d(2), text: b"a".to_vec() },
        Piece::Spelled { key: d(2), text: b"b".to_vec() },
        ident(b"a", None),
        ident(b"a", Some(d(1))),
        ident(b"a", Some(d(2))),
        ident(b"b", None),
    ];
    for (i, a) in ps.iter().enumerate() {
        for (j, b) in ps.iter().enumerate() {
            assert_eq!(a.cmp_key(b), i.cmp(&j), "{a:?} {b:?}");
        }
    }
    let c =
        |file, lo, hi, stance, pieces: Vec<Piece>| Claim { pieces, ..claim(file, lo, hi, stance) };
    let cs = [
        c(0, 9, 9, Stance::Unresolved, vec![]),
        c(0, 10, 11, Stance::Replace, vec![]),
        c(0, 10, 11, Stance::Replace, vec![literal(b"a")]),
        c(0, 10, 11, Stance::Replace, vec![literal(b"a"), Piece::Copy(place(0, 0, 1))]),
        c(0, 10, 11, Stance::Replace, vec![literal(b"b")]),
        c(0, 10, 11, Stance::Keep, vec![]),
        c(0, 10, 11, Stance::Refuse, vec![]),
        c(0, 10, 12, Stance::Replace, vec![]),
        c(1, 0, 1, Stance::Replace, vec![]),
    ];
    for (i, a) in cs.iter().enumerate() {
        for (j, b) in cs.iter().enumerate() {
            assert_eq!(a.cmp_key(b), i.cmp(&j));
        }
    }
    // Not part of the key: subjects, reason, tiers, macro, votes, notes.
    let other = Claim {
        subjects: vec![d(1)],
        tiers: tiers::T1,
        macro_site: macro_site(1, b"M"),
        votes: 9,
        notes: vec![b"n".to_vec()],
        ..cs[2].clone()
    };
    assert!(other.cmp_key(&cs[2]).is_eq());
}

// ---------------------------------------------------------------------------
// Ownership (spec/edits.md §2.1)

fn ownership(owned: &[&[u8]], readonly: &[&[u8]]) -> Ownership {
    Ownership {
        owned: owned.iter().map(|p| p.to_vec()).collect(),
        readonly: readonly.iter().map(|p| p.to_vec()).collect(),
    }
}

// §2.1: an entry covers a path it equals or is a proper prefix of ending
// at a `/`; the longest covering entry decides.
#[test]
fn is_owned() {
    let o = ownership(&[b"/s/a.cc", b"/s/lib"], &[]);
    assert!(o.is_owned(b"/s/a.cc"));
    assert!(o.is_owned(b"/s/lib/x.h"));
    assert!(o.is_owned(b"/s/lib"));
    // A prefix that ends inside a component covers nothing.
    assert!(!o.is_owned(b"/s/a.ccx"));
    assert!(!o.is_owned(b"/s/a.cc.h"));
    assert!(!o.is_owned(b"/s/library/x.h"));
    assert!(!o.is_owned(b"/s"));
    assert!(!o.is_owned(b"/t/a.cc"));
    // readonly inside owned inside readonly.
    let o = ownership(&[b"/s/src"], &[b"/s", b"/s/src/gen"]);
    assert!(!o.is_owned(b"/s/x.h"));
    assert!(o.is_owned(b"/s/src/x.h"));
    assert!(o.is_owned(b"/s/src/general.h"));
    assert!(!o.is_owned(b"/s/src/gen"));
    assert!(!o.is_owned(b"/s/src/gen/x.h"));
    assert!(!o.is_owned(b"/s/src/gen/deeper/x.h"));
    // owned inside readonly inside owned.
    let o = ownership(&[b"/s", b"/s/v/ours"], &[b"/s/v"]);
    assert!(o.is_owned(b"/s/a.cc"));
    assert!(!o.is_owned(b"/s/v/a.h"));
    assert!(o.is_owned(b"/s/v/ours/a.h"));
    // The root covers every absolute path, and nothing relative.
    let o = ownership(&[b"/"], &[b"/usr"]);
    assert!(o.is_owned(b"/a.cc"));
    assert!(o.is_owned(b"/"));
    assert!(!o.is_owned(b"/usr/include/x.h"));
    assert!(!o.is_owned(b"/usr"));
    assert!(o.is_owned(b"/usrx/a.h"));
    assert!(!o.is_owned(b"a.cc"));
    assert!(!o.is_owned(b""));
    // An entry is a prefix followed by `/`, never a prefix of a component.
    let o = ownership(&[b"/a/b"], &[]);
    assert!(!o.is_owned(b"/a/bc/x"));
    assert!(o.is_owned(b"/a/b/x"));
    assert!(covers(b"/", b"/x") && !covers(b"/", b"x"));
    assert!(covers(b"/a/b", b"/a/b") && covers(b"/a/b", b"/a/b/c/d"));
    assert!(!covers(b"/a/b", b"/a/bc/x") && !covers(b"/a/b", b"/a"));
    // One reading only: an entry ending in `/` (not canonical, so never in
    // a valid request) covers nothing but itself.
    assert!(!covers(b"/a/", b"/a/b") && covers(b"/a/", b"/a/"));
    // A carve-out: readonly inside owned, with the file and the directory.
    let o = ownership(&[b"/s"], &[b"/s/gen"]);
    assert!(!o.is_owned(b"/s/gen/x.h"));
    assert!(!o.is_owned(b"/s/gen"));
    assert!(o.is_owned(b"/s/generated/x.h"));
    assert!(o.is_owned(b"/s/x.h"));
}

// spec/edits.md §2.1: a canonical absolute path starts with `/`, has no
// empty, `.` or `..` component, no NUL byte, and no trailing `/` unless it
// is `/`; a path of a file is one that is not `/`.
#[test]
fn canonical_paths() {
    for ok in [&b"/"[..], b"/s", b"/s/a.cc", b"/s/.x", b"/s/..x", b"/s/x..", b"/\xff/b", b"/a b"] {
        assert!(is_canonical(ok), "{}", String::from_utf8_lossy(ok));
    }
    for bad in [
        &b""[..],
        b"s",
        b"s/a",
        b"./s",
        b"//",
        b"//s",
        b"/s/",
        b"/s//a",
        b"/s/./a",
        b"/s/.",
        b"/.",
        b"/..",
        b"/s/../t",
        b"/s/..",
        b"/s/a\0b",
        b"/\0",
        b"\0",
    ] {
        assert!(!is_canonical(bad), "{}", String::from_utf8_lossy(bad));
        assert!(!is_file_path(bad), "{}", String::from_utf8_lossy(bad));
    }
    assert!(!is_file_path(b"/"));
    assert!(is_file_path(b"/s") && is_file_path(b"/s/a.cc"));
}

// ---------------------------------------------------------------------------
// The transform request TRFM (spec/edits.md §3)

fn rename_request() -> Transform {
    Transform {
        kind: TransformKind::Rename,
        ownership: ownership(&[b"/s"], &[b"/s/gen"]),
        indivisible: true,
        rename: Some(Rename {
            target: vec![b"n".to_vec(), b"f".to_vec()],
            kind: TargetKind::Function,
            new_name: b"g".to_vec(),
        }),
        script: None,
    }
}

fn script_request() -> Transform {
    Transform {
        kind: TransformKind::Script,
        ownership: ownership(&[b"/s/a", b"/s/\xffb"], &[]),
        indivisible: false,
        rename: None,
        script: Some(Script {
            id: d(0x77),
            file: b"/t/move.cc".to_vec(),
            options: vec![b"--to=acme".to_vec(), b"\xff".to_vec(), b"--to=acme".to_vec()],
            steps: 1 << 53,
            bytes: 1 << 30,
            depth: 512,
        }),
    }
}

/// The independent encoder of a request, with the marks of its fields.
fn raw_transform(t: &Transform) -> (Vec<u8>, Vec<(String, usize)>) {
    const S: &str = "sections.transform";
    let mut r = Raw::new();
    let list = |r: &mut Raw, at: &str, list: &[Vec<u8>]| {
        r.mark(at.to_owned());
        r.e.count(list.len());
        for (i, b) in list.iter().enumerate() {
            r.mark(format!("{at}[{i}]"));
            r.e.bytes(b);
        }
    };
    r.e.uv(t.kind as u64);
    list(&mut r, &format!("{S}.ownership.owned"), &t.ownership.owned);
    list(&mut r, &format!("{S}.ownership.readonly"), &t.ownership.readonly);
    r.mark(format!("{S}.indivisible"));
    r.e.bool(t.indivisible);
    r.mark(format!("{S}.rename"));
    r.opt(t.rename.as_ref(), |r, n| {
        list(r, &format!("{S}.rename.target"), &n.target);
        r.e.uv(n.kind as u64);
        r.mark(format!("{S}.rename.new_name"));
        r.e.bytes(&n.new_name);
    });
    r.mark(format!("{S}.script"));
    r.opt(t.script.as_ref(), |r, s| {
        r.e.digest(&s.id).bytes(&s.file).count(s.options.len());
        for o in &s.options {
            r.e.bytes(o);
        }
        r.e.uv(s.steps).uv(s.bytes).uv(s.depth);
    });
    let mut w = Writer::new(*b"TRFM", 1);
    w.section(1, r.e.as_bytes());
    (w.finish(), r.marks)
}

const RENAME_TRAILER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn rename_bytes() -> Vec<u8> {
    reseal(hex(&[
        "53 4c 49 50 1a 0a 00 00  54 52 46 4d  01 00 00 00", // magic, TRFM, version 1
        "01 18",                                             // transform, 24 bytes:
        "01",                                                //   rename                        @18
        "01  02 2f 73",                                      //   owned ["/s"]                  @19
        "01  06 2f 73 2f 67 65 6e",                          // readonly ["/s/gen"]         @23
        "01",                                                //   indivisible                   @31
        "01",                                                //   rename:                       @32
        "02  01 6e  01 66",                                  //     target ["n", "f"]           @33
        "01",                                                //     function                    @38
        "01 67",                                             //     new_name "g"                @39
        "00",                                                //   no script                     @41
        "00",                                                // end                             @42
        RENAME_TRAILER, // the BLAKE3 of the rest, written by reseal
    ]))
}

// spec/edits.md §3, spec/model.md §2, §5: a rename request, byte for byte.
#[test]
fn transform_bytes_exact() {
    let bytes = rename_bytes();
    assert_eq!(bytes.len(), 75);
    assert_eq!(rename_request().to_bytes(), bytes);
    assert_eq!(raw_transform(&rename_request()).0, bytes);
    assert_eq!(Transform::from_bytes(&bytes).unwrap(), rename_request());
    assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[43..]);
}

// spec/edits.md §3, spec/model.md §10: round trips of both kinds, goldens.
#[test]
fn transform_round_trip() {
    for (name, t) in
        [("transform-rename.json", rename_request()), ("transform-script.json", script_request())]
    {
        t.validate().unwrap();
        let bytes = t.to_bytes();
        assert_eq!(bytes, raw_transform(&t).0);
        assert_eq!(Transform::from_bytes(&bytes).unwrap(), t);
        let text = dump(&bytes).unwrap();
        golden(name, &text);
        assert_eq!(text, json::write_pretty(&t.to_json()).unwrap());
        assert_eq!(Transform::from_json(&parse(&text)).unwrap(), t);
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[bytes.len() - 32..]);
    }
    // Every target kind.
    for kind in [TargetKind::Any, TargetKind::Variable] {
        let mut t = rename_request();
        t.rename.as_mut().unwrap().kind = kind;
        assert_eq!(Transform::from_bytes(&t.to_bytes()).unwrap(), t);
    }
    // An indivisible script; a script with no option and readonly paths.
    let mut t = script_request();
    t.indivisible = true;
    t.ownership.readonly = vec![b"/s/a/gen".to_vec()];
    t.script.as_mut().unwrap().options.clear();
    assert_eq!(Transform::from_json(&t.to_json()).unwrap(), t);
    assert_eq!(Transform::from_bytes(&t.to_bytes()).unwrap(), t);
}

/// The offset at which `from_bytes` rejects `t`, checked to be that of
/// `path`, with `validate`, `from_json` and `undump` agreeing.
fn refused(t: &Transform, path: &str) -> &'static str {
    let (bytes, marks) = raw_transform(t);
    let e = Transform::from_bytes(&bytes).expect_err("from_bytes accepted an invalid request");
    assert_eq!(e.kind, ErrorKind::Malformed, "{e}");
    let (_, off) = marks.iter().find(|(p, _)| p == path).unwrap();
    assert_eq!(e.offset, payload_at(&bytes, 1) + off, "offset of {path}: {}", e.what);
    let v = t.validate().expect_err("validate accepted an invalid request");
    assert_eq!(v, format!("{path}: {}", e.what));
    let j = Transform::from_json(&t.to_json()).unwrap_err();
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: e.what.to_owned() });
    assert_eq!(undump(&t.to_json()).unwrap_err(), j);
    e.what
}

// spec/edits.md §2.1: owned not empty, both lists strictly ascending, no
// entry in both.
#[test]
fn transform_ownership_rules() {
    let with = |owned: &[&[u8]], readonly: &[&[u8]]| Transform {
        ownership: ownership(owned, readonly),
        ..rename_request()
    };
    refused(&with(&[], &[]), "sections.transform.ownership.owned");
    // Every refused form, in owned and in readonly.
    for bad in [
        &b""[..],
        b"s",
        b"s/a",
        b"//s",
        b"/s/",
        b"/s//a",
        b"/s/./gen",
        b"/s/../t",
        b"/s/..",
        b"/s\0",
    ] {
        let what = refused(&with(&[b"/a", bad], &[]), "sections.transform.ownership.owned[1]");
        assert_eq!(what, "not a canonical absolute path");
        let what = refused(&with(&[b"/s"], &[bad]), "sections.transform.ownership.readonly[0]");
        assert_eq!(what, "not a canonical absolute path");
    }
    // The root is canonical; it owns every absolute path.
    let t = with(&[b"/"], &[b"/usr"]);
    t.validate().unwrap();
    assert_eq!(Transform::from_bytes(&t.to_bytes()).unwrap(), t);
    // Canonical before ascending: the first bad entry, though entry 1 is
    // out of order.
    refused(&with(&[b"/t", b"/s", b"/u/"], &[]), "sections.transform.ownership.owned[2]");
    refused(&with(&[b"/s"], &[b"/u", b"/t", b"x"]), "sections.transform.ownership.readonly[2]");
    // Owned's rules before readonly's.
    refused(&with(&[b"/s/"], &[b"x"]), "sections.transform.ownership.owned[0]");
    refused(&with(&[b"/t", b"/s"], &[]), "sections.transform.ownership.owned[1]");
    refused(&with(&[b"/s", b"/s"], &[]), "sections.transform.ownership.owned[1]");
    refused(&with(&[b"/s"], &[b"/u", b"/t"]), "sections.transform.ownership.readonly[1]");
    refused(&with(&[b"/s", b"/u"], &[b"/t", b"/u"]), "sections.transform.ownership.readonly[1]");
    // In field order: owned before readonly.
    refused(&with(&[b"/t", b"/s"], &[b"/u", b"/t"]), "sections.transform.ownership.owned[1]");
}

// spec/edits.md §3: a rename is indivisible, has a rename and no script;
// a script has a script and no rename.
#[test]
fn transform_kind_rules() {
    refused(
        &Transform { indivisible: false, ..rename_request() },
        "sections.transform.indivisible",
    );
    refused(&Transform { rename: None, ..rename_request() }, "sections.transform.rename");
    refused(
        &Transform { script: script_request().script, ..rename_request() },
        "sections.transform.script",
    );
    refused(
        &Transform { rename: rename_request().rename, ..script_request() },
        "sections.transform.rename",
    );
    refused(&Transform { script: None, ..script_request() }, "sections.transform.script");
    // A rename's own rules before the script's presence.
    let mut t = Transform { script: script_request().script, ..rename_request() };
    t.rename.as_mut().unwrap().new_name = b"f".to_vec();
    refused(&t, "sections.transform.rename.new_name");
}

// spec/edits.md §3: every component of the target and the new name is an
// identifier and not a keyword; the new name is not the last component.
#[test]
fn transform_rename_names() {
    let with = |target: &[&[u8]], new_name: &[u8]| {
        let mut t = rename_request();
        let r = t.rename.as_mut().unwrap();
        r.target = target.iter().map(|c| c.to_vec()).collect();
        r.new_name = new_name.to_vec();
        t
    };
    refused(&with(&[], b"g"), "sections.transform.rename.target");
    for bad in [&b""[..], b"1f", b"f-g", b"f g", b"::f", b"f\xc3\xa9", b"\xff"] {
        let what = refused(&with(&[b"n", bad], b"g"), "sections.transform.rename.target[1]");
        assert!(what.contains("identifier"), "{what}");
        refused(&with(&[b"n", b"f"], bad), "sections.transform.rename.new_name");
    }
    for kw in [
        &b"int"[..],
        b"export",
        b"nullptr",
        b"char8_t",
        b"co_await",
        b"contract_assert",
        b"and",
        b"xor_eq",
        b"register",
    ] {
        let what = refused(&with(&[kw, b"f"], b"g"), "sections.transform.rename.target[0]");
        assert!(what.contains("keyword"), "{what}");
        refused(&with(&[b"f"], kw), "sections.transform.rename.new_name");
    }
    let what = refused(&with(&[b"n", b"f"], b"f"), "sections.transform.rename.new_name");
    assert!(what.contains("last component"), "{what}");
    // Identifiers with special meaning are not keywords; names that only
    // look like keywords are identifiers; the new name may equal an outer
    // component.
    for ok in [
        &b"final"[..],
        b"override",
        b"import",
        b"module",
        b"Int",
        b"int_",
        b"_",
        b"__x",
        b"pre",
        b"post",
    ] {
        with(&[ok, b"f"], b"g").validate().unwrap();
        with(&[b"f"], ok).validate().unwrap();
    }
    with(&[b"n", b"f"], b"n").validate().unwrap();
    // Every component is checked, the first bad one reported.
    refused(&with(&[b"a", b"1", b"int"], b"g"), "sections.transform.rename.target[1]");
}

// spec/model.md §1 rule 6, §2, §5.4: the request's enumerations, its bool
// and opt tags; the section is required; unknown sections.
#[test]
fn transform_structural() {
    let bytes = rename_bytes();
    for (at, from, to) in [(18, 1, 3), (18, 1, 0), (31, 1, 2), (32, 1, 2), (38, 1, 3), (41, 0, 2)] {
        assert_eq!(
            error(Transform::from_bytes(&patch(&bytes, at, from, to))),
            (ErrorKind::Malformed, at),
            "{at}"
        );
    }
    // No section: Malformed at the end byte.
    let w = Writer::new(*b"TRFM", 1).finish();
    assert_eq!(error(Transform::from_bytes(&w)), (ErrorKind::Malformed, 16));
    // Another critical section; an ignorable one is skipped.
    let (_, _) = raw_transform(&rename_request());
    let payload = &bytes[18..42];
    let mut w = Writer::new(*b"TRFM", 1);
    w.section(1, payload);
    w.section(2, &[]);
    assert_eq!(error(Transform::from_bytes(&w.finish())).0, ErrorKind::UnknownSection);
    let mut w = Writer::new(*b"TRFM", 1);
    w.section(1, payload);
    w.section(64, b"x");
    let with_extra = w.finish();
    assert_eq!(Transform::from_bytes(&with_extra).unwrap(), rename_request());
    assert_eq!(undump(&parse(&dump(&with_extra).unwrap())).unwrap(), with_extra);
    // The kind is checked: a shard is not a request.
    assert_eq!(error(Transform::from_bytes(&small_bytes())), (ErrorKind::BadMagic, 8));
    // JSON: names, the bool, the kind.
    for (path, new, at) in [
        (&["sections", "transform", "kind"][..], "\"Rename\"", "sections.transform.kind"),
        (&["sections", "transform", "indivisible"][..], "1", "sections.transform.indivisible"),
        (
            &["sections", "transform", "rename", "kind"][..],
            "\"method\"",
            "sections.transform.rename.kind",
        ),
        (
            &["sections", "transform", "ownership", "owned", "0"][..],
            "{\"hex\": \"2f\"}",
            "sections.transform.ownership.owned[0].hex",
        ),
    ] {
        let mut v = rename_request().to_json();
        set(&mut v, path, parse(new));
        assert_eq!(Transform::from_json(&v).unwrap_err().path, at, "{at}");
    }
    let mut v = rename_request().to_json();
    set(&mut v, &["kind"], parse("\"DECL\""));
    assert_eq!(Transform::from_json(&v).unwrap_err().path, "kind");
}
