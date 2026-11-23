//! The edit set `EDTS` (`spec/edits.md` §6), the overlay `OVLY` (§7.1),
//! the result `RSLT` (§8.4, §8.5) and the transform run record `RUNT`
//! (§10.6), in binary and JSON form, with the geometry of §5.4 and
//! `basis_id`. Work package WP-061; the offsets, paths, messages, orders
//! and JSON forms pinned here are `spec/landed/edits/WP-061.md`. Each test
//! names the section it covers.

use std::fmt::Debug;
use std::fs;
use std::path::Path;

use slip_model::container::{TRAILER_LEN, Writer};
use slip_model::edits::{
    Failure, FailureKind, HostRefusal, MacroSite, Piece, Place, Reason, Report, Role, Severity,
    SourcePlace, Subject, tiers,
};
use slip_model::editset::{
    Basis, Cause, Edit, EditSet, Exempt, Failed, Refusal, Reported, SetState, basis_id, cross,
    properly_inside, share_byte, within,
};
use slip_model::formats::{JsonFormError, content_id, dump, transform_run_names, undump};
use slip_model::hash::{content_digest, tagged_digest};
use slip_model::json::{self, Value};
use slip_model::keys::{KeyKind, Name, Parent, Rule};
use slip_model::overlay::{Content, Obligation, Overlay, OverlayFile};
use slip_model::result::{
    Category, FailedSpelling, Residue, RunVerdict, Spellings, Stale, State, Tally, TransformResult,
    Why,
};
use slip_model::transform_run::{Step, StepName, TransformRun};
use slip_model::wire::{Encoder, Error, ErrorKind};
use slip_model::{Counter, Digest, FileRef};

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

fn members<'v>(v: &'v mut Value, path: &[&str]) -> &'v mut Vec<(String, Value)> {
    let mut cur = v;
    for step in path {
        cur = match cur {
            Value::Object(m) => &mut m.iter_mut().find(|(k, _)| k == step).unwrap().1,
            _ => panic!("members: no {step}"),
        };
    }
    match cur {
        Value::Object(m) => m,
        _ => panic!("members: not an object"),
    }
}

fn error(r: Result<impl Debug, Error>) -> (ErrorKind, usize) {
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
// A second, independent encoder (spec/edits.md §6, §7.1, §8.5, §10.6;
// spec/model.md §2), used to build files that break an invariant
// (`to_bytes` debug-asserts validity), to cross-check `to_bytes`, and to
// find the offset of every field an invariant names: `marks` maps the JSON
// path of a field or element to the offset of its first byte in the
// payload of section 1.

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

    fn opt_digest(&mut self, v: Option<&Digest>) {
        self.opt(v, |r, d| {
            r.e.digest(d);
        });
    }

    fn place(&mut self, p: &Place) {
        self.e.uv(p.file).uv(p.lo).uv(p.hi);
    }

    fn digests(&mut self, list: &[Digest]) {
        self.e.count(list.len());
        for x in list {
            self.e.digest(x);
        }
    }

    fn macro_site(&mut self, m: Option<&MacroSite>) {
        self.opt(m, |r, m| {
            r.e.uv(m.name_start).bytes(&m.name);
        });
    }

    /// `observers: uv, first: list<digest>`, marking `first` at `at.first`.
    fn counts(&mut self, at: &str, observers: u64, first: &[Digest]) {
        self.e.uv(observers);
        self.mark(format!("{at}.first"));
        self.digests(first);
    }

    fn file(self, kind: [u8; 4]) -> Vec<u8> {
        let mut w = Writer::new(kind, version_of(kind));
        w.section(1, self.e.as_bytes());
        w.finish()
    }
}

/// The version this crate writes of a kind of this file: `RSLT` is at 2
/// (WP-111), the others at 1.
fn version_of(kind: [u8; 4]) -> u32 {
    if kind == *b"RSLT" { 2 } else { 1 }
}

#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §6")]
fn raw_set(x: &EditSet) -> Raw {
    const S: &str = "sections.set";
    let mut r = Raw::new();
    r.mark(format!("{S}.scheme"));
    r.e.uv(x.scheme).digest(&x.plan).uv(x.round).digest(&x.transform).digest(&x.basis);
    r.mark(format!("{S}.state"));
    r.e.uv(x.state as u64);
    r.e.count(x.files.len());
    for (i, f) in x.files.iter().enumerate() {
        r.mark(format!("{S}.files[{i}]"));
        r.e.bytes(&f.path).uv(f.size).digest(&f.content);
    }
    r.e.count(x.subjects.len());
    for (i, s) in x.subjects.iter().enumerate() {
        r.mark(format!("{S}.subjects[{i}]"));
        r.e.digest(&s.id);
        r.mark(format!("{S}.subjects[{i}].rule"));
        r.opt(s.rule.as_ref(), |r, rule| rule.encode(&mut r.e));
        r.e.uv(s.role as u64);
        r.mark(format!("{S}.subjects[{i}].of"));
        r.digests(&s.of);
        r.mark(format!("{S}.subjects[{i}].flags"));
        r.e.uv(s.flags).str(&s.display);
    }
    r.e.count(x.rules.len());
    for (i, rule) in x.rules.iter().enumerate() {
        r.mark(format!("{S}.rules[{i}]"));
        rule.encode(&mut r.e);
    }
    r.e.count(x.edits.len());
    for (i, t) in x.edits.iter().enumerate() {
        let at = format!("{S}.edits[{i}]");
        r.mark(at.clone());
        r.e.uv(t.file).uv(t.lo).uv(t.hi).count(t.pieces.len());
        for (j, p) in t.pieces.iter().enumerate() {
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
            }
        }
        r.mark(format!("{at}.subjects"));
        r.digests(&t.subjects);
        r.mark(format!("{at}.tiers"));
        r.e.uv(t.tiers);
        r.macro_site(t.macro_site.as_ref());
        r.mark(format!("{at}.notes"));
        r.e.count(t.notes.len());
        for n in &t.notes {
            r.e.bytes(n);
        }
        r.e.uv(t.observers);
        r.counts(&at, t.votes, &t.first);
    }
    r.e.count(x.groups.len());
    for (i, g) in x.groups.iter().enumerate() {
        r.mark(format!("{S}.groups[{i}]"));
        r.e.count(g.len());
        for &m in g {
            r.e.uv(m);
        }
    }
    r.e.count(x.changed.len());
    for (i, c) in x.changed.iter().enumerate() {
        let at = format!("{S}.changed[{i}]");
        r.mark(at.clone());
        r.e.digest(&c.key);
        r.place(&c.place);
        r.e.bytes(&c.reason);
        r.mark(format!("{at}.in_force"));
        r.e.bool(c.in_force);
        r.counts(&at, c.observers, &c.first);
    }
    for (name, list) in [("kept", &x.kept), ("unresolved", &x.unresolved)] {
        r.e.count(list.len());
        for (i, p) in list.iter().enumerate() {
            r.mark(format!("{S}.{name}[{i}]"));
            r.place(p);
        }
    }
    r.e.count(x.refusals.len());
    for (i, f) in x.refusals.iter().enumerate() {
        let at = format!("{S}.refusals[{i}]");
        r.mark(at.clone());
        r.e.uv(f.cause as u64);
        r.mark(format!("{at}.file"));
        r.opt(f.file.as_ref(), |r, &n| {
            r.e.uv(n);
        });
        r.e.uv(f.lo).uv(f.hi);
        r.mark(format!("{at}.reason"));
        r.e.uv(f.reason as u64);
        r.mark(format!("{at}.subject"));
        r.opt_digest(f.subject.as_ref());
        r.mark(format!("{at}.macro"));
        r.macro_site(f.macro_site.as_ref());
        r.mark(format!("{at}.other"));
        r.opt(f.other.as_ref(), Raw::place);
        r.mark(format!("{at}.observers"));
        r.counts(&at, f.observers, &f.first);
    }
    r.e.count(x.reports.len());
    for (i, p) in x.reports.iter().enumerate() {
        let at = format!("{S}.reports[{i}]");
        r.mark(at.clone());
        r.opt(p.report.place.as_ref(), Raw::place);
        r.e.uv(p.report.severity as u64).bytes(&p.report.message).bool(p.report.attached);
        r.counts(&at, p.observers, &p.first);
    }
    r.e.count(x.failures.len());
    for (i, f) in x.failures.iter().enumerate() {
        let at = format!("{S}.failures[{i}]");
        r.mark(at.clone());
        let x = &f.failure;
        r.e.uv(x.kind as u64).str(&x.message);
        r.opt(x.place.as_ref(), |r, p| {
            r.e.bytes(&p.file).uv(p.line).uv(p.column);
        });
        r.e.str(&x.rendered);
        r.opt(x.refusal.as_ref(), |r, h| {
            r.e.str(&h.function).str(&h.errc).str(&h.message);
            r.mark(format!("{at}.failure.refusal.subject"));
            r.opt(h.subject.as_ref(), Raw::place);
        });
        r.counts(&at, f.observers, &f.first);
    }
    r.digests(&x.unparsed);
    r.e.count(x.counters.len());
    for (i, c) in x.counters.iter().enumerate() {
        r.mark(format!("{S}.counters[{i}]"));
        r.mark(format!("{S}.counters[{i}].name"));
        r.e.str(&c.name).uv(c.value);
    }
    r
}

// The edit's `first` is marked by `counts` after `votes`; `observers` comes
// before `votes` in the record, written above.

fn raw_overlay(x: &Overlay) -> Raw {
    const S: &str = "sections.overlay";
    let mut r = Raw::new();
    r.e.digest(&x.edits);
    r.mark(format!("{S}.files"));
    r.e.count(x.files.len());
    for (i, f) in x.files.iter().enumerate() {
        r.mark(format!("{S}.files[{i}]"));
        r.e.bytes(&f.path).uv(f.pre.size).digest(&f.pre.content);
        r.e.uv(f.post.size).digest(&f.post.content);
    }
    r.e.count(x.spellings.len());
    for (i, o) in x.spellings.iter().enumerate() {
        r.mark(format!("{S}.spellings[{i}]"));
        r.e.uv(o.file).uv(o.lo).uv(o.hi).digest(&o.key);
    }
    r
}

const TALLY: [&str; 11] = [
    "selected",
    "unparsed",
    "failed_runs",
    "error_reports",
    "verified",
    "unchanged",
    "mismatch",
    "not_parsed",
    "unverifiable",
    "never_parsed",
    "failed",
];

fn tally_values(t: &Tally) -> [u64; 11] {
    [
        t.selected,
        t.unparsed,
        t.failed_runs,
        t.error_reports,
        t.verified,
        t.unchanged,
        t.mismatch,
        t.not_parsed,
        t.unverifiable,
        t.never_parsed,
        t.failed,
    ]
}

fn raw_result(x: &TransformResult) -> Raw {
    const S: &str = "sections.result";
    let mut r = Raw::new();
    r.mark(format!("{S}.scheme"));
    r.e.uv(x.scheme).digest(&x.plan).uv(x.round).digest(&x.transform).digest(&x.edits);
    r.mark(format!("{S}.declared"));
    r.opt_digest(x.declared.as_ref());
    r.mark(format!("{S}.overlay"));
    r.opt_digest(x.overlay.as_ref());
    r.mark(format!("{S}.state"));
    r.e.uv(x.state as u64).bool(x.partial).bool(x.allow_unparsed);
    r.mark(format!("{S}.verdict"));
    r.opt(x.verdict.as_ref(), |r, &v| {
        r.e.uv(v as u64);
    });
    for (name, v) in TALLY.iter().zip(tally_values(&x.tally)) {
        r.mark(format!("{S}.tally.{name}"));
        r.e.uv(v);
    }
    r.e.count(x.agreement.len());
    for (i, a) in x.agreement.iter().enumerate() {
        r.mark(format!("{S}.agreement[{i}]"));
        r.e.uv(a.rule).digest(&a.observer).uv(a.direct).uv(a.kept_via);
    }
    r.e.count(x.unused.len());
    for (i, &u) in x.unused.iter().enumerate() {
        r.mark(format!("{S}.unused[{i}]"));
        r.e.uv(u);
    }
    r.mark(format!("{S}.spellings.obligations"));
    r.e.uv(x.spellings.obligations).uv(x.spellings.passed).count(x.spellings.failed.len());
    for (i, f) in x.spellings.failed.iter().enumerate() {
        r.mark(format!("{S}.spellings.failed[{i}]"));
        r.e.uv(f.obligation).uv(f.why as u64);
        r.mark(format!("{S}.spellings.failed[{i}].observers"));
        r.e.uv(f.observers);
        r.mark(format!("{S}.spellings.failed[{i}].first"));
        r.opt_digest(f.first.as_ref());
        r.mark(format!("{S}.spellings.failed[{i}].found"));
        r.opt_digest(f.found.as_ref());
        r.e.str(&f.name);
    }
    r.e.count(x.residue.len());
    for (i, e) in x.residue.iter().enumerate() {
        r.mark(format!("{S}.residue[{i}]"));
        r.e.uv(e.category as u64);
        r.mark(format!("{S}.residue[{i}].file"));
        r.e.bytes(&e.file);
        r.mark(format!("{S}.residue[{i}].lo"));
        r.e.uv(e.lo).uv(e.hi);
        r.mark(format!("{S}.residue[{i}].line"));
        r.e.uv(e.line);
        r.mark(format!("{S}.residue[{i}].column"));
        r.e.uv(e.column);
    }
    r.e.count(x.exempt.len());
    for (i, k) in x.exempt.iter().enumerate() {
        r.mark(format!("{S}.exempt[{i}]"));
        r.e.digest(k);
    }
    r
}

fn raw_run(x: &TransformRun) -> Raw {
    let mut r = Raw::new();
    r.e.str(&x.tool_slip).str(&x.tool_kiln).digest(&x.plan).uv(x.round).digest(&x.transform);
    for o in [&x.edits, &x.declared, &x.overlay, &x.result] {
        r.opt_digest(o.as_ref());
    }
    r.e.uv(x.started_unix_ns).uv(x.wall_ns).count(x.steps.len());
    for s in &x.steps {
        r.e.uv(s.name as u64).uv(s.wall_ns).uv(s.cpu_ns).uv(s.peak_rss);
        r.opt_digest(s.run.as_ref());
    }
    r
}

/// The interface `format_impl!` gives each of the four kinds, and the raw
/// encoder of each.
trait Kind: Sized + Debug + PartialEq + Clone {
    const TAG: [u8; 4];
    fn raw(&self) -> Raw;
    fn read(b: &[u8]) -> Result<Self, Error>;
    fn bytes(&self) -> Vec<u8>;
    fn json(&self) -> Value;
    fn unjson(v: &Value) -> Result<Self, JsonFormError>;
    fn check(&self) -> Result<(), String>;
}

macro_rules! kind {
    ($ty:ty, $tag:literal, $raw:ident) => {
        impl Kind for $ty {
            const TAG: [u8; 4] = *$tag;
            fn raw(&self) -> Raw {
                $raw(self)
            }
            fn read(b: &[u8]) -> Result<Self, Error> {
                <$ty>::from_bytes(b)
            }
            fn bytes(&self) -> Vec<u8> {
                self.to_bytes()
            }
            fn json(&self) -> Value {
                self.to_json()
            }
            fn unjson(v: &Value) -> Result<Self, JsonFormError> {
                <$ty>::from_json(v)
            }
            fn check(&self) -> Result<(), String> {
                self.validate()
            }
        }
    };
}

kind!(EditSet, b"EDTS", raw_set);
kind!(Overlay, b"OVLY", raw_overlay);
kind!(TransformResult, b"RSLT", raw_result);
kind!(TransformRun, b"RUNT", raw_run);

fn raw_file<T: Kind>(x: &T) -> Vec<u8> {
    x.raw().file(T::TAG)
}

/// The file offset of the field at `path` in `raw_file(x)`.
fn at<T: Kind>(x: &T, path: &str) -> usize {
    let bytes = raw_file(x);
    let base = payload_at(&bytes, 1);
    let r = x.raw();
    let (_, off) = r
        .marks
        .iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("the raw encoder marks no {path}"));
    base + off
}

/// Binary and JSON round trips of a valid value: `to_bytes` agrees with
/// the raw encoder, `from_bytes`, `content_id`, `dump`/`undump` and
/// `from_json` give it back.
fn round_trip<T: Kind>(x: &T) {
    x.check().unwrap();
    let bytes = x.bytes();
    assert_eq!(bytes, raw_file(x), "to_bytes and the raw encoder disagree");
    assert_eq!(&T::read(&bytes).unwrap(), x);
    let id = content_id(&bytes).unwrap();
    assert_eq!(id.as_bytes(), &bytes[bytes.len() - TRAILER_LEN..]);
    let text = dump(&bytes).unwrap();
    let v = parse(&text);
    assert_eq!(v, x.json());
    assert_eq!(undump(&v).unwrap(), bytes);
    assert_eq!(&T::unjson(&v).unwrap(), x);
}

/// The message with which `from_bytes` rejects `x`'s file, after checking
/// that it is `Malformed` at the offset of `path` and that `validate`,
/// `from_json` and `undump` reject it at `path` with the same message.
fn rejected<T: Kind>(x: &T, path: &str) -> String {
    let e = T::read(&raw_file(x)).expect_err("from_bytes accepted an invalid value");
    assert_eq!(e.kind, ErrorKind::Malformed, "{e}");
    assert_eq!(e.offset, at(x, path), "offset of {path}: {}", e.what);
    let v = x.check().expect_err("validate accepted an invalid value");
    assert_eq!(v, format!("{path}: {}", e.what), "validate");
    let j = T::unjson(&x.json()).expect_err("from_json accepted an invalid value");
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: e.what.to_owned() });
    assert_eq!(undump(&x.json()).unwrap_err(), j);
    assert!(content_id(&raw_file(x)).is_err());
    e.what.to_owned()
}

// ---------------------------------------------------------------------------
// Samples

fn rule(from: &[u8], to: &[u8]) -> Rule {
    Rule {
        parent: Parent::Root,
        kind: KeyKind::Function,
        name: Name::Identifier(from.to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(to.to_vec()),
    }
}

fn edit(file: u64, lo: u64, hi: u64) -> Edit {
    Edit {
        file,
        lo,
        hi,
        pieces: Vec::new(),
        subjects: Vec::new(),
        tiers: tiers::T0,
        macro_site: None,
        notes: Vec::new(),
        observers: 1,
        votes: 1,
        first: Vec::new(),
    }
}

fn refusal(cause: Cause, file: Option<u64>, lo: u64, hi: u64) -> Refusal {
    Refusal {
        cause,
        file,
        lo,
        hi,
        reason: if cause == Cause::SiteRefused { Reason::MacroBody } else { Reason::None },
        subject: None,
        macro_site: None,
        other: None,
        observers: 0,
        first: Vec::new(),
    }
}

/// An accepted edit set with every record at least once: three files (one
/// path not UTF-8), a seed with a rule and a follower, a rule, an edit
/// with each piece, one nested in it at a macro argument, one of exactly
/// the range the first copies, a group, a `changed` entry, kept and
/// unresolved ranges, refusals of a mixed set, of a site and contested by
/// another range, reports with and without a place, failures with and
/// without a place and a host refusal, an unparsed observer and counters.
#[allow(clippy::too_many_lines, reason = "one value per record of spec/edits.md §6")]
fn sample() -> EditSet {
    EditSet {
        scheme: 1,
        plan: d(0x01),
        round: 2,
        transform: d(0x02),
        basis: d(0x03),
        state: SetState::Accepted,
        files: vec![
            file(b"/s/a.cc", 100, 0x51),
            file(b"/s/a.h", 60, 0x52),
            file(b"/s/\xffb.h", 40, 0x53),
        ],
        subjects: vec![
            Subject {
                id: d(0x10),
                rule: Some(rule(b"f", b"g")),
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
                flags: 1,
                display: "n::f2".to_owned(),
            },
        ],
        rules: vec![rule(b"f", b"g"), rule(b"h", b"i")],
        edits: vec![
            Edit {
                pieces: vec![
                    Piece::Literal(b"x".to_vec()),
                    Piece::Copy(place(1, 0, 5)),
                    Piece::Spelled { key: d(0x10), text: b"g".to_vec() },
                ],
                subjects: vec![d(0x10), d(0x11)],
                notes: vec![b"a".to_vec(), b"b".to_vec()],
                observers: 2,
                votes: 2,
                first: vec![d(0xa0), d(0xa1)],
                ..edit(0, 10, 20)
            },
            Edit {
                pieces: vec![Piece::Literal(b"y".to_vec())],
                tiers: tiers::T0 | tiers::T1,
                macro_site: Some(MacroSite { name_start: 11, name: b"M".to_vec() }),
                ..edit(0, 12, 14)
            },
            edit(1, 0, 5),
        ],
        groups: vec![vec![0, 2]],
        changed: vec![Exempt {
            key: d(0x20),
            place: place(0, 10, 20),
            reason: b"r".to_vec(),
            in_force: true,
            observers: 1,
            first: vec![d(0xa0)],
        }],
        kept: vec![place(0, 30, 33), place(1, 6, 7)],
        unresolved: vec![place(2, 1, 4)],
        refusals: vec![
            Refusal { subject: Some(d(0x12)), ..refusal(Cause::MixedSet, Some(0), 50, 55) },
            Refusal {
                subject: Some(d(0x10)),
                macro_site: Some(MacroSite { name_start: 40, name: b"N".to_vec() }),
                observers: 1,
                first: vec![d(0xa1)],
                ..refusal(Cause::SiteRefused, Some(0), 40, 45)
            },
            Refusal {
                other: Some(place(1, 11, 13)),
                observers: 1,
                first: vec![d(0xa0)],
                ..refusal(Cause::Contested, Some(1), 10, 12)
            },
        ],
        reports: vec![
            Reported {
                report: Report {
                    place: None,
                    severity: Severity::Note,
                    message: b"m".to_vec(),
                    attached: false,
                },
                observers: 1,
                first: vec![d(0xa0)],
            },
            Reported {
                report: Report {
                    place: Some(place(0, 1, 2)),
                    severity: Severity::Review,
                    message: b"w".to_vec(),
                    attached: true,
                },
                observers: 2,
                first: vec![d(0xa0), d(0xa1)],
            },
        ],
        failures: vec![
            Failed {
                failure: Failure {
                    kind: FailureKind::Uncaught,
                    message: "boom".to_owned(),
                    place: None,
                    rendered: "boom\n".to_owned(),
                    refusal: Some(HostRefusal {
                        function: "slip::rename".to_owned(),
                        errc: "not_owned".to_owned(),
                        message: "no".to_owned(),
                        subject: Some(place(2, 0, 3)),
                    }),
                },
                observers: 1,
                first: vec![d(0xa1)],
            },
            Failed {
                failure: Failure {
                    kind: FailureKind::StepBudget,
                    message: "steps".to_owned(),
                    place: Some(SourcePlace { file: b"/t/x.cc".to_vec(), line: 3, column: 4 }),
                    rendered: String::new(),
                    refusal: None,
                },
                observers: 3,
                first: vec![d(0xa0), d(0xa1), d(0xa2)],
            },
        ],
        unparsed: vec![d(0xb0)],
        counters: vec![
            Counter { name: "api.unresolved_spelling_matches".to_owned(), value: 3 },
            Counter { name: "reduce.edits".to_owned(), value: 3 },
        ],
    }
}

/// A refused edit set: no edits, no groups, so no `changed` entry in
/// force; a refusal for want of an observer, one of a subject, `no_edit`.
fn refused() -> EditSet {
    let mut changed = sample().changed;
    changed[0].in_force = false;
    EditSet {
        state: SetState::Refused,
        edits: Vec::new(),
        groups: Vec::new(),
        changed,
        refusals: vec![
            refusal(Cause::NoObserver, None, 0, 0),
            Refusal { subject: Some(d(0x10)), ..refusal(Cause::NameTaken, None, 0, 0) },
            refusal(Cause::NoEdit, None, 0, 0),
        ],
        ..sample()
    }
}

fn content(size: u64, n: u8) -> Content {
    Content { size, content: d(n) }
}

/// Three files: one with two obligations, one unchanged with none, one
/// with one.
fn overlay() -> Overlay {
    Overlay {
        edits: d(0x04),
        files: vec![
            OverlayFile {
                path: b"/s/a.cc".to_vec(),
                pre: content(100, 0x51),
                post: content(98, 0x61),
            },
            OverlayFile {
                path: b"/s/a.h".to_vec(),
                pre: content(60, 0x52),
                post: content(60, 0x52),
            },
            OverlayFile {
                path: b"/s/\xffb.h".to_vec(),
                pre: content(40, 0x53),
                post: content(41, 0x63),
            },
        ],
        spellings: vec![
            Obligation { file: 0, lo: 10, hi: 11, key: d(0x10) },
            Obligation { file: 0, lo: 50, hi: 98, key: d(0x10) },
            Obligation { file: 2, lo: 0, hi: 1, key: d(0x11) },
        ],
    }
}

/// A result with every record: a verdict, the tally, stale rules (by
/// `direct` and by `kept_via`), unused rules, failed spellings of both kinds, residue of every
/// category.
fn result() -> TransformResult {
    TransformResult {
        scheme: 1,
        plan: d(0x01),
        round: 2,
        transform: d(0x02),
        edits: d(0x05),
        declared: Some(d(0x06)),
        overlay: Some(d(0x07)),
        state: State::NotVerified,
        partial: true,
        allow_unparsed: false,
        verdict: Some(RunVerdict::NotVerified),
        tally: Tally {
            selected: 12,
            unparsed: 1,
            failed_runs: 2,
            error_reports: 3,
            verified: 4,
            unchanged: 1,
            mismatch: 1,
            not_parsed: 1,
            unverifiable: 1,
            never_parsed: 1,
            failed: 2,
        },
        agreement: vec![
            Stale { rule: 0, observer: d(0xa1), direct: 2, kept_via: 0 },
            Stale { rule: 1, observer: d(0xa0), direct: 0, kept_via: 3 },
        ],
        unused: vec![1, 4],
        spellings: Spellings {
            obligations: 5,
            passed: 3,
            failed: vec![
                FailedSpelling {
                    obligation: 1,
                    why: Why::DenotesOther,
                    observers: 2,
                    first: Some(d(0xa0)),
                    found: Some(d(0x13)),
                    name: "n::g".to_owned(),
                },
                FailedSpelling {
                    obligation: 4,
                    why: Why::Unseen,
                    observers: 0,
                    first: None,
                    found: None,
                    name: "n::h".to_owned(),
                },
            ],
        },
        residue: vec![
            Residue {
                category: Category::Dependent,
                file: b"/s/a.cc".to_vec(),
                lo: 3,
                hi: 4,
                line: 1,
                column: 4,
            },
            Residue {
                category: Category::Mention,
                file: b"/s/a.cc".to_vec(),
                lo: 9,
                hi: 10,
                line: 2,
                column: 1,
            },
            Residue {
                category: Category::NotCompiled,
                file: b"/s/a.h".to_vec(),
                lo: 0,
                hi: 1,
                line: 1,
                column: 1,
            },
            Residue {
                category: Category::NotPlanned,
                file: b"/s/\xffz.h".to_vec(),
                lo: 5,
                hi: 6,
                line: 1,
                column: 6,
            },
        ],
        exempt: vec![d(0x20), d(0x21)],
    }
}

/// A refused result: no overlay, no declared changes, no verdict, the
/// counts of the map phase only.
fn refused_result() -> TransformResult {
    TransformResult {
        declared: None,
        overlay: None,
        state: State::Refused,
        verdict: None,
        tally: Tally {
            selected: 5,
            unparsed: 1,
            failed_runs: 1,
            error_reports: 2,
            ..Tally::default()
        },
        agreement: Vec::new(),
        unused: Vec::new(),
        spellings: Spellings::default(),
        residue: Vec::new(),
        ..result()
    }
}

/// [`result`] made consistent with the state `s` (rows 16 and 17 of
/// `spec/landed/verify/WP-070.md` §1).
fn result_in(s: State) -> TransformResult {
    let mut x = result();
    x.state = s;
    match s {
        State::Verified | State::VerifiedWithResidue => {
            x.verdict = Some(RunVerdict::Verified);
            x.agreement.clear();
            x.spellings = Spellings { obligations: 5, passed: 5, failed: Vec::new() };
            x.allow_unparsed = true;
            if s == State::Verified {
                x.residue.retain(|r| r.category == Category::Mention);
                x.tally.failed_runs = 0;
                x.tally.error_reports = 0;
            }
        }
        State::Incomplete => x.verdict = Some(RunVerdict::Incomplete),
        State::NotVerified => {}
        State::Refused => return refused_result(),
    }
    x
}

/// A run record with every step name and both forms of each opt.
fn run() -> TransformRun {
    let step = |name, run| Step { name, wall_ns: 7, cpu_ns: 6, peak_rss: 1 << 20, run };
    TransformRun {
        tool_slip: "slip 0.1".to_owned(),
        tool_kiln: "kiln 0.2".to_owned(),
        plan: d(0x01),
        round: 2,
        transform: d(0x02),
        edits: Some(d(0x05)),
        declared: None,
        overlay: Some(d(0x07)),
        result: None,
        started_unix_ns: 1_700_000_000_000_000_000,
        wall_ns: 99,
        steps: vec![
            step(StepName::Load, None),
            step(StepName::Plan, None),
            step(StepName::Map, Some(d(0xc0))),
            step(StepName::Reduce, None),
            step(StepName::Apply, None),
            step(StepName::Verify, Some(d(0xc1))),
            step(StepName::Scan, None),
            step(StepName::Conclude, None),
            step(StepName::Write, None),
        ],
    }
}

// ---------------------------------------------------------------------------
// Hand-written files (spec/model.md §2, §5; spec/edits.md §6, §7.1, §8.5,
// §10.6), derived from the grammar and commented per field.

const HEADER_EDTS: &str = "53 4c 49 50 1a 0a 00 00  45 44 54 53  01 00 00 00"; // magic, EDTS, version 1
const HEADER_OVLY: &str = "53 4c 49 50 1a 0a 00 00  4f 56 4c 59  01 00 00 00"; // magic, OVLY, version 1
const HEADER_RSLT: &str = "53 4c 49 50 1a 0a 00 00  52 53 4c 54  02 00 00 00"; // magic, RSLT, version 2
const HEADER_RUNT: &str = "53 4c 49 50 1a 0a 00 00  52 55 4e 54  01 00 00 00"; // magic, RUNT, version 1
const TRAILER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One accepted edit of `[2, 5)` of `/a` to the literal `xy`.
fn small_set() -> EditSet {
    EditSet {
        scheme: 1,
        plan: d(0x11),
        round: 0,
        transform: d(0x22),
        basis: d(0x33),
        state: SetState::Accepted,
        files: vec![file(b"/a", 10, 0x55)],
        subjects: Vec::new(),
        rules: Vec::new(),
        edits: vec![Edit {
            pieces: vec![Piece::Literal(b"xy".to_vec())],
            first: vec![d(0x66)],
            ..edit(0, 2, 5)
        }],
        groups: Vec::new(),
        changed: Vec::new(),
        kept: Vec::new(),
        unresolved: Vec::new(),
        refusals: Vec::new(),
        reports: Vec::new(),
        failures: Vec::new(),
        unparsed: Vec::new(),
        counters: Vec::new(),
    }
}

fn small_set_bytes() -> Vec<u8> {
    reseal(hex(&[
        HEADER_EDTS,
        "01 c3 01",         // section set, 195 bytes:            @16
        "01",               //   scheme 1                          @19
        &rep("11", 32),     //   plan
        "00",               //   round 0
        &rep("22", 32),     //   transform
        &rep("33", 32),     //   basis
        "00",               //   state accepted                    @117
        "01  02 2f 61  0a", //  files: "/a", 10 bytes            @118
        &rep("55", 32),     //     content
        "00  00",           //   subjects [], rules []             @155
        "01",               //   edits: 1
        "00 02 05",         //     file 0, [2, 5)                  @158
        "01  00 02 78 79",  //     pieces: literal "xy"
        "00",               //     subjects []
        "01",               //     tiers {0}                       @167
        "00",               //     no macro
        "00",               //     notes []
        "01 01",            //     observers 1, votes 1
        "01",               //     first: 1
        &rep("66", 32),
        "00 00 00 00 00 00 00 00 00", // groups, changed, kept, unresolved, refusals, reports, failures, unparsed, counters
        "00",                         // end
        TRAILER,                      // the BLAKE3 of the rest, written by reseal
    ]))
}

/// One unchanged file with one obligation: `post == pre` is valid.
fn small_overlay() -> Overlay {
    Overlay {
        edits: d(0x11),
        files: vec![OverlayFile {
            path: b"/a".to_vec(),
            pre: content(10, 0x22),
            post: content(10, 0x22),
        }],
        spellings: vec![Obligation { file: 0, lo: 1, hi: 2, key: d(0x33) }],
    }
}

fn small_overlay_bytes() -> Vec<u8> {
    reseal(hex(&[
        HEADER_OVLY,
        "01 8a 01",     // section overlay, 138 bytes
        &rep("11", 32), //   edits                             @19
        "01  02 2f 61", //   files: 1, "/a"                    @51
        "0a",           //     pre.size 10
        &rep("22", 32), //     pre.content
        "0a",           //     post.size 10
        &rep("22", 32), //     post.content
        "01  00 01 02", //   spellings: 1, file 0, [1, 2)      @121
        &rep("33", 32), //     key
        "00",           // end
        TRAILER,
    ]))
}

/// A refused result: nothing optional, two counts of the map phase.
fn small_result() -> TransformResult {
    TransformResult {
        scheme: 1,
        plan: d(0x11),
        round: 0,
        transform: d(0x22),
        edits: d(0x33),
        declared: None,
        overlay: None,
        state: State::Refused,
        partial: false,
        allow_unparsed: false,
        verdict: None,
        tally: Tally { selected: 2, unparsed: 1, ..Tally::default() },
        agreement: Vec::new(),
        unused: Vec::new(),
        spellings: Spellings::default(),
        residue: Vec::new(),
        exempt: Vec::new(),
    }
}

fn small_result_bytes() -> Vec<u8> {
    reseal(hex(&[
        HEADER_RSLT,
        "01 7a",                            // section result, 122 bytes
        "01",                               //   scheme 1                          @18
        &rep("11", 32),                     //   plan
        "00",                               //   round 0
        &rep("22", 32),                     //   transform
        &rep("33", 32),                     //   edits
        "00 00",                            //   no declared, no overlay           @116
        "04",                               //   state refused                     @118
        "00 00",                            //   partial false, allow_unparsed false
        "00",                               //   no verdict                        @121
        "02 01 00 00 00 00 00 00 00 00 00", // tally: selected 2, unparsed 1, the rest 0
        "00 00",                            //   agreement [], unused []
        "00 00 00",                         //   spellings: 0 obligations, 0 passed, failed []
        "00 00",                            //   residue [], exempt []
        "00",                               // end
        TRAILER,
    ]))
}

/// The compile step of a run that got as far as reduce.
fn small_run() -> TransformRun {
    TransformRun {
        tool_slip: "s".to_owned(),
        tool_kiln: "k".to_owned(),
        plan: d(0x11),
        round: 2,
        transform: d(0x22),
        edits: Some(d(0x33)),
        declared: None,
        overlay: None,
        result: None,
        started_unix_ns: 128,
        wall_ns: 5,
        steps: vec![Step { name: StepName::Load, wall_ns: 1, cpu_ns: 0, peak_rss: 0, run: None }],
    }
}

fn small_run_bytes() -> Vec<u8> {
    reseal(hex(&[
        HEADER_RUNT,
        "01 72",        // section run, 114 bytes
        "01 73  01 6b", //   tool_slip "s", tool_kiln "k"      @18
        &rep("11", 32), //   plan
        "02",           //   round 2
        &rep("22", 32), //   transform
        "01",           //   edits present                     @87
        &rep("33", 32),
        "00 00 00",       //   no declared, overlay, result
        "80 01",          //   started_unix_ns 128
        "05",             //   wall_ns 5
        "01",             //   steps: 1                          @126
        "08 01 00 00 00", //     load, wall 1, cpu 0, rss 0, no run
        "00",             // end
        TRAILER,
    ]))
}

// spec/edits.md §6, §7.1, §8.5, §10.6, spec/model.md §2, §5: each kind,
// byte for byte.
#[test]
fn bytes_exact() {
    let set = small_set_bytes();
    assert_eq!(set.len(), 16 + 3 + 195 + 1 + 32);
    assert_eq!(small_set().to_bytes(), set);
    assert_eq!(raw_file(&small_set()), set);
    assert_eq!(EditSet::from_bytes(&set).unwrap(), small_set());

    let ov = small_overlay_bytes();
    assert_eq!(ov.len(), 16 + 3 + 138 + 1 + 32);
    assert_eq!(small_overlay().to_bytes(), ov);
    assert_eq!(raw_file(&small_overlay()), ov);
    assert_eq!(Overlay::from_bytes(&ov).unwrap(), small_overlay());

    let res = small_result_bytes();
    assert_eq!(res.len(), 16 + 2 + 122 + 1 + 32);
    assert_eq!(small_result().to_bytes(), res);
    assert_eq!(raw_file(&small_result()), res);
    assert_eq!(TransformResult::from_bytes(&res).unwrap(), small_result());

    let run = small_run_bytes();
    assert_eq!(run.len(), 16 + 2 + 114 + 1 + 32);
    assert_eq!(small_run().to_bytes(), run);
    assert_eq!(raw_file(&small_run()), run);
    assert_eq!(TransformRun::from_bytes(&run).unwrap(), small_run());
}

// spec/edits.md §6, §7.1, §8.5, §10.6, spec/model.md §10: binary and JSON
// round trips of each kind, each with every record.
#[test]
fn round_trips() {
    round_trip(&sample());
    round_trip(&refused());
    round_trip(&small_set());
    round_trip(&overlay());
    round_trip(&small_overlay());
    round_trip(&result());
    round_trip(&refused_result());
    round_trip(&small_result());
    round_trip(&run());
    round_trip(&small_run());
    // Every value of every enumeration.
    for (i, s) in StepName::ALL.into_iter().enumerate() {
        assert_eq!(s as usize, i);
        let mut x = small_run();
        x.steps[0].name = s;
        round_trip(&x);
    }
    for (i, s) in State::ALL.into_iter().enumerate() {
        assert_eq!(s as usize, i);
        round_trip(&result_in(s));
    }
    for (i, v) in RunVerdict::ALL.into_iter().enumerate() {
        assert_eq!(v as usize, i);
        let s = match v {
            RunVerdict::Verified => State::VerifiedWithResidue,
            RunVerdict::NotVerified => State::NotVerified,
            RunVerdict::Incomplete => State::Incomplete,
        };
        let x = result_in(s);
        assert_eq!(x.verdict, Some(v));
        round_trip(&x);
    }
    for (i, c) in Category::ALL.into_iter().enumerate() {
        assert_eq!(c as usize, i);
        let mut x = result();
        x.residue[0].category = c;
        round_trip(&x);
    }
    for (i, w) in Why::ALL.into_iter().enumerate() {
        assert_eq!(w as usize, i);
        // failed[1] is unseen: its observers, first and found are those
        // §8.5 row 6a asks of either kind.
        let mut x = result();
        x.spellings.failed[1] = FailedSpelling { why: w, ..x.spellings.failed[1].clone() };
        if w == Why::DenotesOther {
            x.spellings.failed[1].observers = 1;
            x.spellings.failed[1].first = Some(d(0xa1));
        }
        round_trip(&x);
    }
    for (i, s) in SetState::ALL.into_iter().enumerate() {
        assert_eq!(s as usize, i);
    }
    for (i, c) in Cause::ALL.into_iter().enumerate() {
        assert_eq!(c as usize, i);
        let mut x = refused();
        x.refusals = vec![Refusal {
            reason: if c == Cause::SiteRefused { Reason::Pasted } else { Reason::None },
            subject: (c.is_of_subject() && c != Cause::NoSubject).then_some(d(0x10)),
            ..if c.is_of_site() { refusal(c, Some(0), 1, 2) } else { refusal(c, None, 0, 0) }
        }];
        round_trip(&x);
    }
    // Values at and above 2^53 are decimal strings in JSON.
    let mut x = sample();
    x.files[2].size = u64::MAX;
    x.unresolved[0] = place(2, 1 << 53, u64::MAX);
    x.counters[0].value = u64::MAX;
    round_trip(&x);
    let mut x = run();
    x.wall_ns = u64::MAX;
    round_trip(&x);
}

// spec/model.md §10, spec/landed/edits/WP-061.md §6: the renderings, pinned.
#[test]
fn goldens() {
    fn one<T: Kind>(name: &str, x: &T) {
        let bytes = x.bytes();
        let text = dump(&bytes).unwrap();
        golden(name, &text);
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        assert_eq!(&T::unjson(&parse(&text)).unwrap(), x);
    }
    one("editset.json", &sample());
    one("editset-refused.json", &refused());
    one("overlay.json", &overlay());
    one("result.json", &result());
    one("result-refused.json", &refused_result());
    one("transform-run.json", &run());
}

// spec/edits.md §8.5 (WP-111): `RSLT` is at version 2, and version 1 is
// refused, from bytes (`UnsupportedVersion` at the version field) and from
// JSON (at `version`); version 3 is newer than supported.
#[test]
fn result_version_1_is_refused() {
    let bytes = small_result_bytes();
    assert_eq!(&bytes[12..16], &[2, 0, 0, 0]);
    TransformResult::from_bytes(&bytes).unwrap();
    let payload = &bytes[payload_at(&bytes, 1)..bytes.len() - 33];
    for v in [1u32, 3] {
        let mut w = Writer::new(*b"RSLT", v);
        w.section(1, payload);
        let old = w.finish();
        assert_eq!(
            error(TransformResult::from_bytes(&old)),
            (ErrorKind::UnsupportedVersion, 12),
            "{v}"
        );
    }
    let mut j = small_result().to_json();
    set(&mut j, &["version"], Value::Int(1));
    let e = TransformResult::from_json(&j).unwrap_err();
    assert_eq!(e.path, "version", "{e}");
}

// spec/model.md §5.4: dump, undump and content_id know the four kinds; an
// unknown ignorable section survives dump → undump.
#[test]
fn dump_undump_content_id() {
    for bytes in [small_set_bytes(), small_overlay_bytes(), small_result_bytes(), small_run_bytes()]
    {
        let text = dump(&bytes).unwrap();
        assert!(text.contains(std::str::from_utf8(&bytes[8..12]).unwrap()));
        assert_eq!(undump(&parse(&text)).unwrap(), bytes);
        assert_eq!(content_id(&bytes).unwrap().as_bytes(), &bytes[bytes.len() - 32..]);
        // The same payload with an ignorable section 64 after it.
        let payload = &bytes[payload_at(&bytes, 1)..bytes.len() - 33];
        let mut w = Writer::new(
            bytes[8..12].try_into().unwrap(),
            version_of(bytes[8..12].try_into().unwrap()),
        );
        w.section(1, payload);
        w.section(64, b"\x01\x02");
        let with = w.finish();
        let text = dump(&with).unwrap();
        assert!(text.contains("\"#64\""), "{text}");
        assert_eq!(undump(&parse(&text)).unwrap(), with);
        assert_eq!(content_id(&with).unwrap().as_bytes(), &with[with.len() - 32..]);
        // An unknown critical section is refused by all three.
        let mut w = Writer::new(
            bytes[8..12].try_into().unwrap(),
            version_of(bytes[8..12].try_into().unwrap()),
        );
        w.section(1, payload);
        w.section(2, b"\x01");
        let bad = w.finish();
        assert_eq!(error(content_id(&bad)).0, ErrorKind::UnknownSection);
        assert_eq!(error(dump(&bad)).0, ErrorKind::UnknownSection);
        // No section at all: Malformed at the end byte.
        let w = Writer::new(
            bytes[8..12].try_into().unwrap(),
            version_of(bytes[8..12].try_into().unwrap()),
        );
        let empty = w.finish();
        assert_eq!(error(content_id(&empty)), (ErrorKind::Malformed, 16));
    }
    let mut v = parse(&dump(&small_run_bytes()).unwrap());
    set(&mut v, &["kind"], Value::Str("RUNX".to_owned()));
    let e = undump(&v).unwrap_err();
    assert_eq!(e.path, "kind");
    assert!(e.message.contains("\"RUNT\""), "{e}");
}

// spec/model.md §2: an unlisted enumeration value and an opt tag other
// than 0 or 1 are Malformed at their first byte; a byte after the last
// field is Malformed there; JSON names are checked at their paths.
#[test]
fn unlisted_values() {
    let set_b = small_set_bytes();
    assert_eq!(
        error(EditSet::from_bytes(&patch(&set_b, 117, 0x00, 0x02))),
        (ErrorKind::Malformed, 117)
    );
    assert_eq!(
        error(EditSet::from_bytes(&patch(&set_b, 167, 0x01, 0x00))),
        (ErrorKind::Malformed, 167)
    );
    let res = small_result_bytes();
    assert_eq!(
        error(TransformResult::from_bytes(&patch(&res, 118, 0x04, 0x05))),
        (ErrorKind::Malformed, 118)
    );
    assert_eq!(
        error(TransformResult::from_bytes(&patch(&res, 121, 0x00, 0x02))),
        (ErrorKind::Malformed, 121)
    );
    let run_b = small_run_bytes();
    assert_eq!(
        error(TransformRun::from_bytes(&patch(&run_b, 127, 0x08, 0x09))),
        (ErrorKind::Malformed, 127)
    );
    assert_eq!(
        error(TransformRun::from_bytes(&patch(&run_b, 87, 0x01, 0x02))),
        (ErrorKind::Malformed, 87)
    );
    // The refusal cause, the category, why, a verdict: past the last.
    let mut x = refused();
    x.refusals.truncate(1);
    let bytes = raw_file(&x);
    let off = at(&x, "sections.set.refusals[0]");
    assert_eq!(
        error(EditSet::from_bytes(&patch(&bytes, off, 0x00, 18))),
        (ErrorKind::Malformed, off)
    );
    let x = result();
    let bytes = raw_file(&x);
    let off = at(&x, "sections.result.residue[0]");
    assert_eq!(
        error(TransformResult::from_bytes(&patch(&bytes, off, 0x00, 4))),
        (ErrorKind::Malformed, off)
    );
    let off = at(&x, "sections.result.spellings.failed[0]") + 1;
    assert_eq!(
        error(TransformResult::from_bytes(&patch(&bytes, off, 0x00, 2))),
        (ErrorKind::Malformed, off)
    );
    let off = at(&x, "sections.result.verdict") + 1;
    assert_eq!(
        error(TransformResult::from_bytes(&patch(&bytes, off, 0x01, 3))),
        (ErrorKind::Malformed, off)
    );
    // A byte after the last field of the section.
    for (bytes, kind) in [
        (small_set_bytes(), *b"EDTS"),
        (small_overlay_bytes(), *b"OVLY"),
        (small_result_bytes(), *b"RSLT"),
        (small_run_bytes(), *b"RUNT"),
    ] {
        let start = payload_at(&bytes, 1);
        let mut payload = bytes[start..bytes.len() - 33].to_vec();
        payload.push(0);
        let mut w = Writer::new(kind, version_of(kind));
        w.section(1, &payload);
        let long = w.finish();
        assert_eq!(
            error(content_id(&long)),
            (ErrorKind::Malformed, payload_at(&long, 1) + payload.len() - 1)
        );
    }
    // JSON names.
    let cases: [(Value, &[&str], &str); 6] = [
        (small_set().to_json(), &["sections", "set", "state"], "sections.set.state"),
        (
            refused().to_json(),
            &["sections", "set", "refusals", "0", "cause"],
            "sections.set.refusals[0].cause",
        ),
        (result().to_json(), &["sections", "result", "state"], "sections.result.state"),
        (result().to_json(), &["sections", "result", "verdict"], "sections.result.verdict"),
        (
            result().to_json(),
            &["sections", "result", "residue", "0", "category"],
            "sections.result.residue[0].category",
        ),
        (run().to_json(), &["sections", "run", "steps", "0", "name"], "sections.run.steps[0].name"),
    ];
    for (mut v, steps, path) in cases {
        set(&mut v, steps, Value::Str("Accepted".to_owned()));
        let e = undump(&v).unwrap_err();
        assert_eq!(e.path, path);
        assert!(e.message.starts_with("expected"), "{e}");
    }
    let mut v = result().to_json();
    set(
        &mut v,
        &["sections", "result", "spellings", "failed", "1", "why"],
        Value::Str("seen".to_owned()),
    );
    assert_eq!(undump(&v).unwrap_err().path, "sections.result.spellings.failed[1].why");
}

// spec/landed/edits/WP-061.md §6: the JSON members, in order; a missing,
// extra or misplaced member is an error at its path.
#[test]
#[allow(clippy::many_single_char_names, reason = "one short name per record of §6")]
#[allow(clippy::too_many_lines, reason = "one block per kind, one statement per record")]
fn json_members() {
    let v = sample().to_json();
    let s = v.get("sections").unwrap().get("set").unwrap();
    let keys =
        |v: &Value| v.as_object().unwrap().iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    assert_eq!(
        keys(s),
        [
            "scheme",
            "plan",
            "round",
            "transform",
            "basis",
            "state",
            "files",
            "subjects",
            "rules",
            "edits",
            "groups",
            "changed",
            "kept",
            "unresolved",
            "refusals",
            "reports",
            "failures",
            "unparsed",
            "counters"
        ]
    );
    let first = |name: &str| s.get(name).unwrap().as_array().unwrap()[0].clone();
    assert_eq!(
        keys(&first("edits")),
        [
            "file",
            "lo",
            "hi",
            "pieces",
            "subjects",
            "tiers",
            "macro",
            "notes",
            "observers",
            "votes",
            "first"
        ]
    );
    assert_eq!(
        keys(&first("changed")),
        ["key", "place", "reason", "in_force", "observers", "first"]
    );
    assert_eq!(
        keys(&first("refusals")),
        ["cause", "file", "lo", "hi", "reason", "subject", "macro", "other", "observers", "first"]
    );
    assert_eq!(keys(&first("reports")), ["report", "observers", "first"]);
    assert_eq!(keys(&first("failures")), ["failure", "observers", "first"]);
    assert_eq!(keys(&first("counters")), ["name", "value"]);
    assert_eq!(s.get("state").unwrap(), &Value::Str("accepted".to_owned()));
    assert_eq!(first("refusals").get("cause").unwrap(), &Value::Str("mixed_set".to_owned()));
    assert_eq!(first("refusals").get("file").unwrap(), &Value::Int(0));
    assert_eq!(first("refusals").get("other").unwrap(), &Value::Null);

    let v = overlay().to_json();
    let o = v.get("sections").unwrap().get("overlay").unwrap();
    assert_eq!(keys(o), ["edits", "files", "spellings"]);
    let f = o.get("files").unwrap().as_array().unwrap()[0].clone();
    assert_eq!(keys(&f), ["path", "pre", "post"]);
    assert_eq!(keys(f.get("pre").unwrap()), ["size", "content"]);
    assert_eq!(
        keys(&o.get("spellings").unwrap().as_array().unwrap()[0]),
        ["file", "lo", "hi", "key"]
    );

    let v = result().to_json();
    let r = v.get("sections").unwrap().get("result").unwrap();
    assert_eq!(
        keys(r),
        [
            "scheme",
            "plan",
            "round",
            "transform",
            "edits",
            "declared",
            "overlay",
            "state",
            "partial",
            "allow_unparsed",
            "verdict",
            "tally",
            "agreement",
            "unused",
            "spellings",
            "residue",
            "exempt"
        ]
    );
    assert_eq!(keys(r.get("tally").unwrap()), TALLY);
    assert_eq!(
        keys(&r.get("agreement").unwrap().as_array().unwrap()[0]),
        ["rule", "observer", "direct", "kept_via"]
    );
    let sp = r.get("spellings").unwrap();
    assert_eq!(keys(sp), ["obligations", "passed", "failed"]);
    assert_eq!(
        keys(&sp.get("failed").unwrap().as_array().unwrap()[0]),
        ["obligation", "why", "observers", "first", "found", "name"]
    );
    assert_eq!(
        keys(&r.get("residue").unwrap().as_array().unwrap()[0]),
        ["category", "file", "lo", "hi", "line", "column"]
    );
    assert_eq!(r.get("state").unwrap(), &Value::Str("not_verified".to_owned()));
    assert_eq!(r.get("verdict").unwrap(), &Value::Str("not_verified".to_owned()));

    let v = run().to_json();
    let t = v.get("sections").unwrap().get("run").unwrap();
    assert_eq!(
        keys(t),
        [
            "tool_slip",
            "tool_kiln",
            "plan",
            "round",
            "transform",
            "edits",
            "declared",
            "overlay",
            "result",
            "started_unix_ns",
            "wall_ns",
            "steps"
        ]
    );
    let step = t.get("steps").unwrap().as_array().unwrap()[0].clone();
    assert_eq!(keys(&step), ["name", "wall_ns", "cpu_ns", "peak_rss", "run"]);
    assert_eq!(step.get("name").unwrap(), &Value::Str("load".to_owned()));
    assert_eq!(t.get("started_unix_ns").unwrap(), &Value::Str("1700000000000000000".to_owned()));

    // A missing member, an extra one, one out of order.
    let mut v = run().to_json();
    let m = members(&mut v, &["sections", "run"]);
    m.retain(|(k, _)| k != "wall_ns");
    let e = TransformRun::from_json(&v).unwrap_err();
    assert_eq!((e.path.as_str(), e.message.as_str()), ("sections.run.wall_ns", "missing member"));
    let mut v = overlay().to_json();
    let m = members(&mut v, &["sections", "overlay"]);
    m.push(("extra".to_owned(), Value::Null));
    let e = Overlay::from_json(&v).unwrap_err();
    assert_eq!((e.path.as_str(), e.message.as_str()), ("sections.overlay.extra", "unknown member"));
    let mut v = result().to_json();
    let m = members(&mut v, &["sections", "result"]);
    m.swap(0, 1);
    let e = TransformResult::from_json(&v).unwrap_err();
    assert_eq!(
        (e.path.as_str(), e.message.as_str()),
        ("sections.result.scheme", "member out of order")
    );
    // A section under another name.
    let mut v = sample().to_json();
    let m = members(&mut v, &["sections"]);
    m[0].0 = "edits".to_owned();
    assert_eq!(EditSet::from_json(&v).unwrap_err().path, "sections.edits");
}

// ---------------------------------------------------------------------------
// The edit set's invariants (spec/edits.md §6.1)

/// Every file index of `x`'s records (row 12's list), for a test that
/// inserts a file.
fn shift_files(x: &mut EditSet) -> Vec<&mut u64> {
    let mut v: Vec<&mut u64> = Vec::new();
    for e in &mut x.edits {
        v.push(&mut e.file);
        for p in &mut e.pieces {
            if let Piece::Copy(c) = p {
                v.push(&mut c.file);
            }
        }
    }
    v.extend(x.changed.iter_mut().map(|c| &mut c.place.file));
    v.extend(x.kept.iter_mut().chain(&mut x.unresolved).map(|p| &mut p.file));
    for r in &mut x.refusals {
        v.extend(r.file.as_mut());
        v.extend(r.other.as_mut().map(|p| &mut p.file));
    }
    v.extend(x.reports.iter_mut().filter_map(|r| r.report.place.as_mut()).map(|p| &mut p.file));
    for f in &mut x.failures {
        if let Some(p) = f.failure.refusal.as_mut().and_then(|r| r.subject.as_mut()) {
            v.push(&mut p.file);
        }
    }
    v
}

fn broken(f: impl FnOnce(&mut EditSet), path: &str) -> String {
    let mut x = sample();
    f(&mut x);
    rejected(&x, path)
}

// The row numbers below are those of `spec/landed/edits/WP-061.md` §2.

// §6.1 row 1: scheme is 1.
#[test]
fn set_scheme() {
    for scheme in [0, 2] {
        assert_eq!(broken(|x| x.scheme = scheme, "sections.set.scheme"), "unknown edit set scheme");
    }
}

// §6.1 rows 2 and 3: files canonical, then strictly ascending by path.
#[test]
fn set_files() {
    for path in [&b"s/a.h"[..], b"/s/./a.h", b"/s//a.h", b"/s/a.h/", b"/s/../a.h", b"", b"/s/a\0.h"]
    {
        let what = broken(|x| x.files[1].path = path.to_vec(), "sections.set.files[1]");
        assert_eq!(what, "not a canonical absolute path");
    }
    // `/` is canonical (an ownership entry) but names no file.
    let what = broken(
        |x| {
            x.files.insert(0, file(b"/", 0, 0x50));
            for f in shift_files(x) {
                *f += 1;
            }
        },
        "sections.set.files[0]",
    );
    assert_eq!(what, "the root / is not a file");
    let what = broken(|x| x.files[1].path = b"/s/a.cc".to_vec(), "sections.set.files[1]");
    assert_eq!(what, "files are not strictly ascending by path");
    let what = broken(|x| x.files.swap(1, 2), "sections.set.files[2]");
    assert_eq!(what, "files are not strictly ascending by path");
    // Canonical is checked over the whole list first.
    let what = broken(
        |x| {
            x.files.swap(0, 1);
            x.files[2].path = b"/s/x/".to_vec();
        },
        "sections.set.files[2]",
    );
    assert_eq!(what, "not a canonical absolute path");
}

// §6.1 row 4: the subject rules, as in the shard (WP-060): ascending by
// id, then `of`, then the flags, each over the whole list.
#[test]
fn set_subjects() {
    broken(|x| x.subjects.swap(0, 1), "sections.set.subjects[1]");
    broken(|x| x.subjects[1].id = d(0x10), "sections.set.subjects[1]");
    broken(|x| x.subjects[1].of.clear(), "sections.set.subjects[1].of");
    broken(|x| x.subjects[0].of = vec![d(0x10)], "sections.set.subjects[0].of");
    broken(|x| x.subjects[1].of = vec![d(0x10), d(0x10)], "sections.set.subjects[1].of");
    broken(|x| x.subjects[0].flags = 16, "sections.set.subjects[0].flags");
    // `of` of every subject before the flags of any.
    broken(
        |x| {
            x.subjects[0].flags = 16;
            x.subjects[1].of.clear();
        },
        "sections.set.subjects[1].of",
    );
}

// §6.1 row 5: rules as a rule table; a name a reader refuses, in memory.
#[test]
fn set_rules() {
    let what = broken(|x| x.rules[1] = rule(b"f", b"j"), "sections.set.rules[1]");
    assert_eq!(what, "two rules with one (parent, kind, name, disc)");
    let what = broken(|x| x.rules.swap(0, 1), "sections.set.rules[1]");
    assert_eq!(what, "rules are not strictly ascending");
    let mut x = sample();
    x.rules[0].to_name = Name::Operator(0);
    assert!(x.validate().unwrap_err().starts_with("sections.set.rules[0]: "));
    let mut x = sample();
    x.subjects[0].rule.as_mut().unwrap().name = Name::Operator(0);
    assert!(x.validate().unwrap_err().starts_with("sections.set.subjects[0].rule: "));
}

// §6.1 row 6: a refused set has no edits, then no groups.
#[test]
fn set_refused_lists() {
    let what = broken(|x| x.state = SetState::Refused, "sections.set.edits[0]");
    assert_eq!(what, "a refused edit set has edits");
    let what = broken(
        |x| {
            x.state = SetState::Refused;
            x.edits.clear();
        },
        "sections.set.groups[0]",
    );
    assert_eq!(what, "a refused edit set has groups");
}

// §6.1 row 7, §5.4: edits ascending, not empty, nested or disjoint, and
// crossing no copied range. Nested edits and an edit of exactly a copied
// range are valid (the sample has both).
#[test]
fn set_edit_geometry() {
    let what = broken(|x| x.edits.swap(0, 1), "sections.set.edits[1]");
    assert_eq!(what, "edits are not strictly ascending by (file, lo, hi)");
    let what = broken(|x| x.edits[1] = edit(0, 10, 20), "sections.set.edits[1]");
    assert_eq!(what, "edits are not strictly ascending by (file, lo, hi)");
    let what = broken(|x| x.edits[2].hi = 0, "sections.set.edits[2]");
    assert_eq!(what, "an edit's range is empty");
    // Crossing: [10, 20) and [15, 25).
    let what = broken(|x| x.edits[1] = edit(0, 15, 25), "sections.set.edits[1]");
    assert_eq!(what, "an edit crosses another edit");
    // [10, 20) holds [12, 14): [13, 22) crosses both; [15, 22) crosses the
    // outer one once the inner one has closed.
    let what = broken(|x| x.edits.insert(2, edit(0, 13, 22)), "sections.set.edits[2]");
    assert_eq!(what, "an edit crosses another edit");
    let what = broken(|x| x.edits.insert(2, edit(0, 15, 22)), "sections.set.edits[2]");
    assert_eq!(what, "an edit crosses another edit");
    // Touching at one end, and nested at the start and at the end: valid.
    let mut x = sample();
    x.edits.insert(2, edit(0, 20, 22));
    x.edits.insert(0, edit(0, 10, 11));
    x.edits.insert(3, edit(0, 19, 20));
    assert_eq!(
        x.edits.iter().map(|e| (e.file, e.lo, e.hi)).collect::<Vec<_>>(),
        [(0, 10, 11), (0, 10, 20), (0, 12, 14), (0, 19, 20), (0, 20, 22), (1, 0, 5)]
    );
    x.groups = vec![vec![1, 5]];
    x.validate().unwrap();
    round_trip(&x);
    // An edit in another file at the same offsets does not cross.
    let mut x = sample();
    x.edits.insert(2, edit(1, 0, 4));
    x.edits.push(edit(2, 15, 25));
    x.groups = vec![vec![0, 3]];
    x.validate().unwrap();
    // A copied range [0, 5) of file 1: an edit ending inside it, one
    // starting inside it.
    let what = broken(|x| x.edits[2] = edit(1, 3, 8), "sections.set.edits[2]");
    assert_eq!(what, "an edit crosses a range an edit copies");
    let what = broken(
        |x| {
            x.edits[0].pieces[1] = Piece::Copy(place(1, 2, 8));
        },
        "sections.set.edits[2]",
    );
    assert_eq!(what, "an edit crosses a range an edit copies");
    // An edit crossing the range its own piece copies.
    let what =
        broken(|x| x.edits[0].pieces[1] = Piece::Copy(place(0, 5, 15)), "sections.set.edits[0]");
    assert_eq!(what, "an edit crosses a range an edit copies");
    // Inside a copied range, or containing one: valid.
    let mut x = sample();
    x.edits[2] = edit(1, 1, 3);
    x.validate().unwrap();
    x.edits[2] = edit(1, 0, 9);
    x.validate().unwrap();
    // The least index: the copy check of an edit comes before the next
    // edit's crossing.
    let what = broken(
        |x| {
            x.edits[1] = edit(0, 15, 25);
            x.edits[0].pieces[1] = Piece::Copy(place(0, 5, 15));
        },
        "sections.set.edits[0]",
    );
    assert_eq!(what, "an edit crosses a range an edit copies");
}

// §6.1 row 8: the piece rules, for each edit in turn.
#[test]
fn set_pieces() {
    broken(
        |x| x.edits[0].pieces[0] = Piece::Literal(Vec::new()),
        "sections.set.edits[0].pieces[0]",
    );
    broken(
        |x| x.edits[0].pieces.insert(1, Piece::Literal(b"z".to_vec())),
        "sections.set.edits[0].pieces[1]",
    );
    broken(
        |x| x.edits[0].pieces[2] = Piece::Spelled { key: d(0x10), text: Vec::new() },
        "sections.set.edits[0].pieces[2]",
    );
    broken(
        |x| x.edits[0].pieces[1] = Piece::Copy(place(1, 3, 3)),
        "sections.set.edits[0].pieces[1]",
    );
    broken(
        |x| x.edits[0].pieces[1] = Piece::Copy(place(1, 55, 61)),
        "sections.set.edits[0].pieces[1]",
    );
    broken(
        |x| x.edits[0].pieces[1] = Piece::Copy(place(3, 0, 1)),
        "sections.set.edits[0].pieces[1]",
    );
    broken(
        |x| x.edits[1].pieces[0] = Piece::Literal(Vec::new()),
        "sections.set.edits[1].pieces[0]",
    );
}

// §6.1 row 9: an edit's subjects and notes strictly ascending, its tiers
// those of a replace claim.
#[test]
fn set_edit_fields() {
    let what = broken(|x| x.edits[0].subjects.swap(0, 1), "sections.set.edits[0].subjects");
    assert_eq!(what, "an edit's subjects are not strictly ascending");
    broken(|x| x.edits[0].subjects[1] = d(0x10), "sections.set.edits[0].subjects");
    let what = broken(|x| x.edits[0].notes.swap(0, 1), "sections.set.edits[0].notes");
    assert_eq!(what, "an edit's notes are not strictly ascending");
    let what = broken(|x| x.edits[0].tiers = 0, "sections.set.edits[0].tiers");
    assert_eq!(what, "tiers is 0");
    broken(|x| x.edits[0].tiers = 32, "sections.set.edits[0].tiers");
    let what = broken(|x| x.edits[0].tiers = tiers::T0 | tiers::T2, "sections.set.edits[0].tiers");
    assert!(what.contains("not a refuse"), "{what}");
    let what = broken(
        |x| x.edits[0].macro_site = Some(MacroSite { name_start: 0, name: b"M".to_vec() }),
        "sections.set.edits[0].tiers",
    );
    assert_eq!(what, "a claim of tier 0 alone has a macro");
    // Subjects, then notes, then tiers, for one edit; every field of an
    // edit before the next edit.
    broken(
        |x| {
            x.edits[0].tiers = 0;
            x.edits[0].notes.swap(0, 1);
        },
        "sections.set.edits[0].notes",
    );
    broken(
        |x| {
            x.edits[0].notes.swap(0, 1);
            x.edits[0].subjects.swap(0, 1);
        },
        "sections.set.edits[0].subjects",
    );
    broken(
        |x| {
            x.edits[1].tiers = 0;
            x.edits[0].notes.swap(0, 1);
        },
        "sections.set.edits[0].notes",
    );
    // Row 9 over every edit before row 10.
    broken(
        |x| {
            x.edits[0].tiers = 0;
            x.edits[1].pieces[0] = Piece::Literal(Vec::new());
        },
        "sections.set.edits[1].pieces[0]",
    );
}

// §6.1 row 10: the group rules, over edits.
#[test]
fn set_groups() {
    let what = broken(|x| x.groups[0] = vec![0], "sections.set.groups[0]");
    assert_eq!(what, "a group has fewer than two members");
    let what = broken(|x| x.groups[0] = vec![2, 0], "sections.set.groups[0]");
    assert_eq!(what, "a group's members are not strictly ascending");
    let what = broken(|x| x.groups[0] = vec![0, 3], "sections.set.groups[0]");
    assert_eq!(what, "a group member is not the index of an edit");
    broken(|x| x.groups[0] = vec![0, u64::MAX], "sections.set.groups[0]");
    let what = broken(|x| x.groups.push(vec![1, 2]), "sections.set.groups[1]");
    assert_eq!(what, "an edit is in two groups");
    let mut x = sample();
    x.groups = vec![vec![1, 2], vec![0, 1]];
    assert_eq!(rejected(&x, "sections.set.groups[1]"), "an edit is in two groups");
    x.groups = vec![vec![1, 2], vec![0, 3]];
    rejected(&x, "sections.set.groups[1]");
    // Two disjoint groups out of order.
    let mut x = sample();
    x.edits.push(edit(2, 0, 1));
    x.groups = vec![vec![1, 3], vec![0, 2]];
    assert_eq!(rejected(&x, "sections.set.groups[1]"), "groups are not strictly ascending");
}

// §6.1 row 11: every place within its file; a changed place not empty; a
// refusal that is not of a site has no range.
#[test]
fn set_places() {
    let what = broken(|x| x.edits[0].hi = 101, "sections.set.edits[0]");
    assert_eq!(what, "an edit's range is not within its file");
    broken(|x| x.edits[2].file = 3, "sections.set.edits[2]");
    let what = broken(|x| x.changed[0].place = place(0, 10, 10), "sections.set.changed[0]");
    assert_eq!(what, "a changed entry's place is empty or not within its file");
    broken(|x| x.changed[0].place = place(3, 0, 1), "sections.set.changed[0]");
    broken(|x| x.changed[0].place = place(0, 99, 101), "sections.set.changed[0]");
    let what = broken(|x| x.kept[1] = place(1, 6, 61), "sections.set.kept[1]");
    assert_eq!(what, "a kept range is not within its file");
    broken(|x| x.kept[1] = place(1, 7, 6), "sections.set.kept[1]");
    let what = broken(|x| x.unresolved[0] = place(2, 1, 41), "sections.set.unresolved[0]");
    assert_eq!(what, "an unresolved range is not within its file");
    let what = broken(
        |x| x.refusals.insert(0, refusal(Cause::NoObserver, None, 1, 0)),
        "sections.set.refusals[0]",
    );
    assert_eq!(what, "a refusal that is not of a site has a range");
    broken(
        |x| x.refusals.insert(0, refusal(Cause::NoObserver, None, 0, 1)),
        "sections.set.refusals[0]",
    );
    let what = broken(|x| x.refusals[1].hi = 101, "sections.set.refusals[1]");
    assert_eq!(what, "a refusal's range is not within its file");
    broken(|x| x.refusals[1].file = Some(3), "sections.set.refusals[1]");
    let what = broken(|x| x.refusals[2].other = Some(place(1, 59, 61)), "sections.set.refusals[2]");
    assert_eq!(what, "a refusal's other range is not within its file");
    broken(|x| x.refusals[0].other = Some(place(3, 0, 0)), "sections.set.refusals[0]");
    let what =
        broken(|x| x.reports[1].report.place = Some(place(0, 1, 101)), "sections.set.reports[1]");
    assert_eq!(what, "a report's place is not within its file");
    let what = broken(
        |x| x.failures[0].failure.refusal.as_mut().unwrap().subject = Some(place(2, 0, 41)),
        "sections.set.failures[0].failure.refusal.subject",
    );
    assert_eq!(what, "the refusal's subject is not within its file");
    // Empty places are valid where the spec does not forbid them.
    let mut x = sample();
    x.kept[0] = place(0, 30, 30);
    x.reports[1].report.place = Some(place(0, 100, 100));
    x.validate().unwrap();
    // In field order: edits before changed, kept before unresolved.
    broken(
        |x| {
            x.unresolved[0] = place(2, 1, 41);
            x.kept[1] = place(1, 6, 61);
        },
        "sections.set.kept[1]",
    );
    broken(
        |x| {
            x.changed[0].place = place(0, 10, 10);
            x.edits[2].hi = 61;
        },
        "sections.set.edits[2]",
    );
}

// §6.1 row 14: first at most 8 and at most observers; notes at most 8.
#[test]
fn set_first_and_notes() {
    let nine = || (0..9u8).map(|i| vec![b'a' + i]).collect::<Vec<_>>();
    let many = |n: u8| (0..n).map(|i| d(0xd0 + i)).collect::<Vec<_>>();
    let what = broken(|x| x.edits[0].notes = nine(), "sections.set.edits[0].notes");
    assert_eq!(what, "notes has more than 8 entries");
    let what = broken(
        |x| {
            x.edits[0].observers = 9;
            x.edits[0].first = many(9);
        },
        "sections.set.edits[0].first",
    );
    assert_eq!(what, "first has more than 8 entries");
    let what = broken(|x| x.edits[0].first = many(3), "sections.set.edits[0].first");
    assert_eq!(what, "first has more entries than observers");
    broken(|x| x.changed[0].first = many(2), "sections.set.changed[0].first");
    broken(|x| x.refusals[0].first = many(1), "sections.set.refusals[0].first");
    broken(|x| x.reports[1].first = many(3), "sections.set.reports[1].first");
    broken(
        |x| {
            x.failures[1].observers = 100;
            x.failures[1].first = many(9);
        },
        "sections.set.failures[1].first",
    );
    // Eight entries are valid.
    let mut x = sample();
    x.edits[0].notes.truncate(0);
    x.edits[0].notes = nine()[..8].to_vec();
    x.edits[0].observers = 8;
    x.edits[0].first = many(8);
    x.validate().unwrap();
    // The edits before the other lists; within an edit, notes then first.
    broken(
        |x| {
            x.changed[0].first = many(2);
            x.edits[1].first = many(2);
        },
        "sections.set.edits[1].first",
    );
    broken(
        |x| {
            x.edits[0].first = many(3);
            x.edits[0].notes = nine();
        },
        "sections.set.edits[0].notes",
    );
    broken(
        |x| {
            x.failures[0].first = many(2);
            x.reports[0].first = many(2);
        },
        "sections.set.reports[0].first",
    );
}

// §6.1 row 15: the six lists strictly ascending in their keys.
#[test]
fn set_order() {
    let what = broken(|x| x.changed.push(x.changed[0].clone()), "sections.set.changed[1]");
    assert_eq!(what, "changed is not strictly ascending by (key, place, reason)");
    // The key is (key, place, reason): the counts are not in it.
    broken(
        |x| {
            let mut c = x.changed[0].clone();
            c.observers = 2;
            x.changed.push(c);
        },
        "sections.set.changed[1]",
    );
    let mut x = sample();
    let mut c = x.changed[0].clone();
    c.reason = b"s".to_vec();
    x.changed.push(c);
    x.validate().unwrap();
    let what = broken(|x| x.kept.swap(0, 1), "sections.set.kept[1]");
    assert_eq!(what, "kept is not strictly ascending");
    let what = broken(|x| x.unresolved.push(place(2, 1, 4)), "sections.set.unresolved[1]");
    assert_eq!(what, "unresolved is not strictly ascending");
    let what = broken(|x| x.refusals.swap(1, 2), "sections.set.refusals[2]");
    assert_eq!(what, "refusals are not strictly ascending by (cause, file, lo, hi, subject)");
    // Equal in the key, different elsewhere.
    broken(
        |x| {
            let mut r = x.refusals[2].clone();
            r.observers = 2;
            r.first.push(d(0xa1));
            x.refusals.push(r);
        },
        "sections.set.refusals[3]",
    );
    // An absent subject before a present one (an absent file before a
    // present one cannot be seen: of one cause, either every refusal has a
    // file or none has, row 17).
    let mut x = sample();
    x.refusals.insert(0, refusal(Cause::MixedSet, Some(0), 50, 55));
    x.validate().unwrap();
    x.refusals.swap(0, 1);
    rejected(&x, "sections.set.refusals[1]");
    // Two refusals that differ only in `hi`.
    let mut x = sample();
    x.refusals.push(refusal(Cause::Overlap, Some(0), 3, 5));
    x.refusals.push(refusal(Cause::Overlap, Some(0), 3, 8));
    x.validate().unwrap();
    x.refusals.swap(3, 4);
    rejected(&x, "sections.set.refusals[4]");
    let what = broken(|x| x.reports.swap(0, 1), "sections.set.reports[1]");
    assert_eq!(what, "reports are not strictly ascending");
    let what = broken(|x| x.failures.swap(0, 1), "sections.set.failures[1]");
    assert_eq!(what, "failures are not strictly ascending by (kind, message, place)");
    // The key is (kind, message, place): rendered and refusal are not in it.
    broken(
        |x| {
            let mut f = x.failures[0].clone();
            f.failure.rendered = "other".to_owned();
            f.failure.refusal = None;
            x.failures.insert(1, f);
        },
        "sections.set.failures[1]",
    );
    // Two failures that differ only in `message`.
    let mut x = sample();
    let mut f = x.failures[1].clone();
    "steps2".clone_into(&mut f.failure.message);
    x.failures.push(f);
    x.validate().unwrap();
    x.failures.swap(1, 2);
    rejected(&x, "sections.set.failures[2]");
    // An absent place before a present one, then by file, line, column.
    let mut x = sample();
    let mut f = x.failures[0].clone();
    f.failure.place = Some(SourcePlace { file: b"/a".to_vec(), line: 9, column: 9 });
    x.failures.insert(1, f);
    x.validate().unwrap();
    x.failures.swap(0, 1);
    rejected(&x, "sections.set.failures[1]");
    let mut x = sample();
    let mut f = x.failures[1].clone();
    f.failure.place = Some(SourcePlace { file: b"/t/x.cc".to_vec(), line: 3, column: 5 });
    x.failures.push(f);
    x.validate().unwrap();
    x.failures.swap(1, 2);
    rejected(&x, "sections.set.failures[2]");
    // The line before the column: (3, 9) before (4, 1).
    let mut x = sample();
    let mut f = x.failures[1].clone();
    f.failure.place = Some(SourcePlace { file: b"/t/x.cc".to_vec(), line: 4, column: 1 });
    x.failures.push(f);
    x.failures[1].failure.place =
        Some(SourcePlace { file: b"/t/x.cc".to_vec(), line: 3, column: 9 });
    x.validate().unwrap();
    x.failures.swap(1, 2);
    rejected(&x, "sections.set.failures[2]");
    // In field order.
    broken(
        |x| {
            x.failures.swap(0, 1);
            x.kept.swap(0, 1);
        },
        "sections.set.kept[1]",
    );
}

// §6.1 row 12: every file is named by a record, each kind of record
// counting.
#[test]
fn set_named_files() {
    let what = broken(|x| x.files.push(file(b"/s/\xffc.h", 1, 0x54)), "sections.set.files[3]");
    assert_eq!(what, "no record names the file");
    // Each kind of record alone names a file: an extra file 3 named by it.
    let namers: [fn(&mut EditSet); 9] = [
        |x| x.edits.push(Edit { pieces: vec![Piece::Literal(b"z".to_vec())], ..edit(3, 0, 1) }),
        |x| x.edits[0].pieces.push(Piece::Copy(place(3, 0, 1))),
        |x| {
            x.changed.push(Exempt {
                place: place(3, 0, 1),
                in_force: false,
                ..x.changed[0].clone()
            });
        },
        |x| x.kept.push(place(3, 0, 1)),
        |x| x.unresolved.push(place(3, 0, 1)),
        |x| x.refusals.push(refusal(Cause::Buried, Some(3), 0, 1)),
        |x| x.refusals[2].other = Some(place(3, 0, 1)),
        |x| {
            x.reports[1].report.place = Some(place(3, 0, 1));
        },
        |x| {
            x.failures[0].failure.refusal.as_mut().unwrap().subject = Some(place(3, 0, 1));
        },
    ];
    for (i, f) in namers.iter().enumerate() {
        let mut x = sample();
        x.files.push(file(b"/s/\xffc.h", 1, 0x54));
        f(&mut x);
        x.validate().unwrap_or_else(|e| panic!("namer {i}: {e}"));
    }
    // A failure's source place is a path, not a file of the set.
    let mut x = sample();
    x.files.push(file(b"/t/x.cc", 1, 0x54));
    rejected(&x, "sections.set.files[3]");
    // For each file in turn, after every place (row 11).
    broken(
        |x| {
            x.files.push(file(b"/s/\xffc.h", 1, 0x54));
            x.kept[1] = place(1, 6, 61);
        },
        "sections.set.kept[1]",
    );
}

// §6.1 row 13, §6.2: a changed entry is in force iff an edit shares a byte
// with its place.
#[test]
fn set_in_force() {
    const WHAT: &str = "in_force is not whether an edit shares a byte with the place";
    let what = broken(|x| x.changed[0].in_force = false, "sections.set.changed[0].in_force");
    assert_eq!(what, WHAT);
    let exempt = |place, in_force| Exempt { place, in_force, ..sample().changed[0].clone() };
    // The edits of file 0 are [10, 20) and [12, 14) inside it; of file 1, [0, 5).
    for (p, in_force) in [
        (place(0, 0, 10), false),  // touching at the start
        (place(0, 20, 30), false), // touching at the end
        (place(0, 0, 11), true),
        (place(0, 19, 25), true),
        (place(0, 15, 16), true), // inside the outer edit, after the inner one
        (place(0, 13, 14), true),
        (place(0, 30, 33), false),
        (place(1, 4, 9), true),
        (place(1, 5, 9), false),
        (place(2, 0, 40), false), // a file with no edit
    ] {
        let mut x = sample();
        x.changed = vec![exempt(p, in_force)];
        x.validate().unwrap_or_else(|e| panic!("{p:?}: {e}"));
        x.changed[0].in_force = !in_force;
        assert_eq!(rejected(&x, "sections.set.changed[0].in_force"), WHAT, "{p:?}");
    }
    // A refused set has no edit: no entry is in force.
    let mut x = refused();
    x.validate().unwrap();
    x.changed[0].in_force = true;
    rejected(&x, "sections.set.changed[0].in_force");
    // For each entry in turn.
    let mut x = sample();
    x.changed.push(exempt(place(0, 30, 33), true));
    x.changed.push(exempt(place(0, 40, 41), true));
    rejected(&x, "sections.set.changed[1].in_force");
    // After the files (row 12).
    broken(
        |x| {
            x.changed[0].in_force = false;
            x.files.push(file(b"/s/\xffc.h", 1, 0x54));
        },
        "sections.set.files[3]",
    );
}

// §6.1 row 16: a refusal's reason is not none iff its cause is site_refused.
#[test]
fn set_refusal_reason() {
    let what = broken(|x| x.refusals[1].reason = Reason::None, "sections.set.refusals[1].reason");
    assert_eq!(what, "reason is none for site_refused or not none for another cause");
    broken(|x| x.refusals[2].reason = Reason::Crossing, "sections.set.refusals[2].reason");
    broken(|x| x.refusals[0].reason = Reason::MacroBody, "sections.set.refusals[0].reason");
}

// §6.1 row 17: a refusal's fields against its cause.
#[test]
#[allow(clippy::too_many_lines, reason = "one block per rule of the row")]
fn set_refusal_fields() {
    let subject_cause = |c: Cause| c.is_of_subject() && c != Cause::NoSubject;
    let alone = [Cause::NoSubject, Cause::NoObserver, Cause::NoEdit, Cause::RuleConflict];
    // A valid refusal of each cause, in a refused set.
    let valid = |c: Cause| Refusal {
        reason: if c == Cause::SiteRefused { Reason::Pasted } else { Reason::None },
        subject: subject_cause(c).then_some(d(0x10)),
        ..if c.is_of_site() { refusal(c, Some(0), 1, 2) } else { refusal(c, None, 0, 0) }
    };
    let one = |r: Refusal| EditSet { refusals: vec![r], ..refused() };
    for c in Cause::ALL {
        one(valid(c)).validate().unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }
    // `file` present iff of a site.
    for c in Cause::ALL {
        let r = if c.is_of_site() {
            Refusal { file: None, lo: 0, hi: 0, ..valid(c) }
        } else {
            Refusal { file: Some(0), lo: 1, hi: 2, ..valid(c) }
        };
        let what = rejected(&one(r), "sections.set.refusals[0].file");
        assert_eq!(what, "file is absent for a refusal of a site or present for another cause");
    }
    // A subject for a refusal of a subject but no_subject; none for the
    // four without one; either for a refusal of a site.
    for c in Cause::ALL.into_iter().filter(|&c| subject_cause(c)) {
        let what = rejected(
            &one(Refusal { subject: None, ..valid(c) }),
            "sections.set.refusals[0].subject",
        );
        assert_eq!(what, "a refusal of a subject has no subject", "{c:?}");
    }
    for c in alone {
        let what = rejected(
            &one(Refusal { subject: Some(d(0x10)), ..valid(c) }),
            "sections.set.refusals[0].subject",
        );
        assert_eq!(
            what,
            "a no_subject, no_observer, no_edit or rule_conflict refusal has a subject"
        );
    }
    for c in Cause::ALL.into_iter().filter(|c| c.is_of_site()) {
        one(Refusal { subject: Some(d(0x10)), ..valid(c) }).validate().unwrap();
    }
    // A macro only for a refusal of a site.
    let m = Some(MacroSite { name_start: 1, name: b"M".to_vec() });
    for c in Cause::ALL {
        let x = one(Refusal { macro_site: m.clone(), ..valid(c) });
        if c.is_of_site() {
            x.validate().unwrap();
        } else {
            let what = rejected(&x, "sections.set.refusals[0].macro");
            assert_eq!(what, "a refusal that is not of a site has a macro");
        }
    }
    // `other` only for contested, overlap, buried and with_group.
    for c in Cause::ALL {
        let x = one(Refusal { other: Some(place(0, 5, 6)), ..valid(c) });
        if matches!(c, Cause::Contested | Cause::Overlap | Cause::Buried | Cause::WithGroup) {
            x.validate().unwrap();
        } else {
            let what = rejected(&x, "sections.set.refusals[0].other");
            assert_eq!(
                what,
                "other is present for a cause other than contested, overlap, buried or with_group"
            );
        }
    }
    // No count for the four without a subject; `first` then fails row 14.
    for c in Cause::ALL {
        let x = one(Refusal { observers: 1, ..valid(c) });
        if alone.contains(&c) {
            let what = rejected(&x, "sections.set.refusals[0].observers");
            assert_eq!(
                what,
                "a no_subject, no_observer, no_edit or rule_conflict refusal has observers"
            );
        } else {
            x.validate().unwrap();
        }
    }
    rejected(
        &one(Refusal { first: vec![d(0xa0)], ..valid(Cause::NoEdit) }),
        "sections.set.refusals[0].first",
    );
    // no_edit: only in a set with no edit, beside no refusal of a replace claim.
    let what =
        broken(|x| x.refusals.push(refusal(Cause::NoEdit, None, 0, 0)), "sections.set.refusals[3]");
    assert_eq!(what, "a no_edit refusal is in a set with an edit");
    for c in Cause::ALL.into_iter().filter(|&c| c != Cause::NoEdit) {
        // In cause order: with_group (17) sorts after no_edit (16).
        let mut refusals = vec![valid(c), valid(Cause::NoEdit)];
        refusals.sort_by_key(|r| r.cause as u8);
        let at = refusals.iter().position(|r| r.cause == Cause::NoEdit).unwrap();
        let x = EditSet { refusals, ..refused() };
        if matches!(
            c,
            Cause::Contested
                | Cause::Divergent
                | Cause::Overlap
                | Cause::FileDiffers
                | Cause::Cyclic
                | Cause::Buried
                | Cause::WithGroup
        ) {
            let what = rejected(&x, &format!("sections.set.refusals[{at}]"));
            assert_eq!(
                what,
                "a no_edit refusal is beside a contested, divergent, overlap, file_differs, cyclic, buried or with_group refusal",
                "{c:?}"
            );
        } else {
            x.validate().unwrap_or_else(|e| panic!("{c:?}: {e}"));
        }
    }
    // The rules in order, for one refusal: file, subject, macro, other, observers.
    let r = Refusal {
        file: Some(0),
        lo: 1,
        hi: 2,
        subject: Some(d(0x10)),
        macro_site: m.clone(),
        other: Some(place(0, 5, 6)),
        observers: 1,
        first: vec![d(0xa0)],
        ..refusal(Cause::NoObserver, None, 0, 0)
    };
    rejected(&one(r.clone()), "sections.set.refusals[0].file");
    let r = Refusal { file: None, lo: 0, hi: 0, ..r };
    rejected(&one(r.clone()), "sections.set.refusals[0].subject");
    let r = Refusal { subject: None, ..r };
    rejected(&one(r.clone()), "sections.set.refusals[0].macro");
    let r = Refusal { macro_site: None, ..r };
    rejected(&one(r.clone()), "sections.set.refusals[0].other");
    let r = Refusal { other: None, ..r };
    rejected(&one(r), "sections.set.refusals[0].observers");
    // For each refusal in turn; after the reasons (row 16).
    let x = EditSet {
        refusals: vec![
            Refusal { observers: 1, ..valid(Cause::NoObserver) },
            Refusal { subject: None, ..valid(Cause::NameTaken) },
        ],
        ..refused()
    };
    rejected(&x, "sections.set.refusals[0].observers");
    let x = EditSet {
        refusals: vec![
            Refusal { file: Some(0), lo: 1, hi: 2, ..valid(Cause::NoObserver) },
            Refusal { reason: Reason::Pasted, ..valid(Cause::NameTaken) },
        ],
        ..refused()
    };
    rejected(&x, "sections.set.refusals[1].reason");
}

// §6.1 row 18: the counter rules of spec/model.md §8.1.
#[test]
fn set_counters() {
    for name in ["", "A", "a..b", ".a", "a.", "a-b"] {
        let what =
            broken(|x| x.counters[0].name = name.to_owned(), "sections.set.counters[0].name");
        assert_eq!(what, "counter name is not [a-z0-9_]+(.[a-z0-9_]+)*");
    }
    let what = broken(|x| x.counters.swap(0, 1), "sections.set.counters[1]");
    assert_eq!(what, "counters are not strictly ascending by name");
    broken(|x| x.counters[1].name = x.counters[0].name.clone(), "sections.set.counters[1]");
    // For each counter in turn: the order of counter 1 before the name of
    // counter 2, the name of counter 1 before the order of counter 2.
    broken(
        |x| {
            x.counters.push(Counter { name: "B".to_owned(), value: 0 });
            x.counters.swap(0, 1);
        },
        "sections.set.counters[1]",
    );
    broken(
        |x| {
            x.counters[1].name = "B".to_owned();
            x.counters.push(Counter { name: "a".to_owned(), value: 0 });
        },
        "sections.set.counters[1].name",
    );
}

// §6.1 row 19: the state is refused when it must be, and a refused set
// has a refusal.
#[test]
fn set_state() {
    const WHAT: &str = "an edit set with no edit, or with a refusal of a subject, no_observer or rule_conflict, is not refused";
    for cause in [
        Cause::NoObserver,
        Cause::NoSubject,
        Cause::SubjectReadOnly,
        Cause::SubjectUnsupported,
        Cause::SubjectUnreachable,
        Cause::NameTaken,
        Cause::MixedFollower,
        Cause::RuleConflict,
    ] {
        let mut x = sample();
        x.refusals = vec![Refusal {
            subject: (cause.is_of_subject() && cause != Cause::NoSubject).then_some(d(0x10)),
            ..refusal(cause, None, 0, 0)
        }];
        assert_eq!(rejected(&x, "sections.set.state"), WHAT, "{cause:?}");
    }
    // The refusals of a site leave a set accepted.
    for cause in Cause::ALL.into_iter().filter(|c| c.is_of_site()) {
        let mut x = sample();
        x.refusals = vec![Refusal {
            reason: if cause == Cause::SiteRefused { Reason::Pasted } else { Reason::None },
            ..refusal(cause, Some(0), 50, 55)
        }];
        x.validate().unwrap();
    }
    let what = broken(
        |x| {
            x.edits.clear();
            x.groups.clear();
            x.changed[0].in_force = false;
        },
        "sections.set.state",
    );
    assert_eq!(what, WHAT);
    // A refused set has a refusal (§5.5).
    let mut x = refused();
    x.refusals.clear();
    assert_eq!(rejected(&x, "sections.set.state"), "a refused edit set has no refusal");
    x.refusals.push(refusal(Cause::Buried, Some(0), 10, 20));
    x.validate().unwrap();
}

/// A change that breaks one rule, and the path of the violation.
type Break<T> = (fn(&mut T), &'static str);

/// One break for each row of §6.1, in the table's order, each alone a
/// violation at its path.
fn set_rows() -> Vec<Break<EditSet>> {
    vec![
        (|x| x.scheme = 0, "sections.set.scheme"),
        (|x| x.files[2].path = b"/s/./b.h".to_vec(), "sections.set.files[2]"),
        (|x| x.files[1].path = b"/s/a.cc".to_vec(), "sections.set.files[1]"),
        (|x| x.subjects[1].of.clear(), "sections.set.subjects[1].of"),
        (|x| x.rules[1] = rule(b"f", b"j"), "sections.set.rules[1]"),
        (|x| x.state = SetState::Refused, "sections.set.edits[0]"),
        (|x| x.edits[2] = edit(1, 3, 8), "sections.set.edits[2]"),
        (|x| x.edits[0].pieces[0] = Piece::Literal(Vec::new()), "sections.set.edits[0].pieces[0]"),
        (|x| x.edits[1].notes = vec![b"b".to_vec(), b"a".to_vec()], "sections.set.edits[1].notes"),
        (|x| x.groups[0] = vec![0], "sections.set.groups[0]"),
        (|x| x.kept[1] = place(1, 6, 61), "sections.set.kept[1]"),
        (|x| x.files.push(file(b"/s/\xffc.h", 1, 0x54)), "sections.set.files[3]"),
        (|x| x.changed[0].in_force = false, "sections.set.changed[0].in_force"),
        (|x| x.refusals[0].first = vec![d(0xd0)], "sections.set.refusals[0].first"),
        (|x| x.unresolved.push(place(2, 1, 4)), "sections.set.unresolved[1]"),
        (|x| x.refusals[2].reason = Reason::Crossing, "sections.set.refusals[2].reason"),
        (|x| x.refusals[1].other = Some(place(0, 1, 2)), "sections.set.refusals[1].other"),
        (|x| "A".clone_into(&mut x.counters[0].name), "sections.set.counters[0].name"),
        (
            |x| {
                x.refusals[0] =
                    Refusal { subject: Some(d(0x12)), ..refusal(Cause::NameTaken, None, 0, 0) };
            },
            "sections.set.state",
        ),
    ]
}

// spec/edits.md §6.1, spec/landed/edits/WP-061.md §2: the rows in order.
// Each break is a violation alone; with any later row's break too, the
// earlier is reported.
#[test]
fn set_order_of_rows() {
    let rows = set_rows();
    for (f, path) in &rows {
        broken(f, path);
    }
    for (i, (f, path)) in rows.iter().enumerate() {
        for (g, later) in &rows[i + 1..] {
            let mut x = sample();
            g(&mut x);
            f(&mut x);
            let v = x.validate().expect_err("two breaks validate");
            assert!(v.starts_with(&format!("{path}: ")), "{path} before {later}: {v}");
            rejected(&x, path);
        }
    }
}

// ---------------------------------------------------------------------------
// The overlay's invariants (spec/edits.md §7.1)

fn broken_overlay(f: impl FnOnce(&mut Overlay), path: &str) -> String {
    let mut x = overlay();
    f(&mut x);
    rejected(&x, path)
}

#[test]
fn overlay_invariants() {
    let what = broken_overlay(
        |x| {
            x.files.clear();
            x.spellings.clear();
        },
        "sections.overlay.files",
    );
    assert_eq!(what, "an overlay has no file");
    for path in [&b"s/a.h"[..], b"/s/./a.h", b"/s/a.h/", b"", b"/s/a.h\0"] {
        let what = broken_overlay(|x| x.files[1].path = path.to_vec(), "sections.overlay.files[1]");
        assert_eq!(what, "not a canonical absolute path");
    }
    let what = broken_overlay(
        |x| {
            x.files.insert(0, OverlayFile { path: b"/".to_vec(), ..x.files[0].clone() });
            for o in &mut x.spellings {
                o.file += 1;
            }
        },
        "sections.overlay.files[0]",
    );
    assert_eq!(what, "the root / is not a file");
    let what = broken_overlay(|x| x.files.swap(0, 1), "sections.overlay.files[1]");
    assert_eq!(what, "files are not strictly ascending by path");
    broken_overlay(|x| x.files[1].path = b"/s/a.cc".to_vec(), "sections.overlay.files[1]");
    let what = broken_overlay(|x| x.spellings.swap(0, 1), "sections.overlay.spellings[1]");
    assert_eq!(what, "spellings are not strictly ascending by (file, lo, hi, key)");
    broken_overlay(|x| x.spellings[1] = x.spellings[0], "sections.overlay.spellings[1]");
    // The key decides between two of one range.
    let mut x = overlay();
    x.spellings.insert(1, Obligation { key: d(0x11), ..x.spellings[0] });
    x.validate().unwrap();
    x.spellings.swap(0, 1);
    rejected(&x, "sections.overlay.spellings[1]");
    let what = broken_overlay(|x| x.spellings[2].file = 3, "sections.overlay.spellings[2]");
    assert_eq!(what, "an obligation's file is not the index of a file");
    broken_overlay(|x| x.spellings[2].file = u64::MAX, "sections.overlay.spellings[2]");
    let what = broken_overlay(|x| x.spellings[0].hi = 10, "sections.overlay.spellings[0]");
    assert_eq!(what, "an obligation's range is empty or not within its post-image");
    broken_overlay(|x| x.spellings[0].hi = 9, "sections.overlay.spellings[0]");
    // Within the post size, not the pre size: 98 is valid, 99 is not.
    broken_overlay(|x| x.spellings[1].hi = 99, "sections.overlay.spellings[1]");
    broken_overlay(|x| x.spellings[2].hi = 42, "sections.overlay.spellings[2]");
    let mut x = overlay();
    x.spellings[2].hi = 41;
    x.validate().unwrap();
    // A file with post equal to pre and no obligation is valid (the
    // sample's second); so is one with an obligation (small_overlay).
    assert_eq!(overlay().files[1].pre, overlay().files[1].post);
    small_overlay().validate().unwrap();
}

#[test]
fn overlay_order_of_checks() {
    let rows: Vec<Break<Overlay>> = vec![
        (|x| x.files[2].path = b"/s/x/".to_vec(), "sections.overlay.files[2]"),
        (|x| x.files[1].path = b"/s/a.cc".to_vec(), "sections.overlay.files[1]"),
        (|x| x.spellings.swap(1, 2), "sections.overlay.spellings[2]"),
        (|x| x.spellings[2].file = 5, "sections.overlay.spellings[2]"),
        (|x| x.spellings[0].hi = 10, "sections.overlay.spellings[0]"),
    ];
    for (i, (f, path)) in rows.iter().enumerate() {
        broken_overlay(f, path);
        for (g, later) in &rows[i + 1..] {
            let mut x = overlay();
            g(&mut x);
            f(&mut x);
            let v = x.validate().expect_err("two breaks validate");
            assert!(v.starts_with(&format!("{path}: ")), "{path} before {later}: {v}");
            rejected(&x, path);
        }
    }
    // Each rule over the whole list: a canonical path of files[2] before
    // the order of files[1]; the range of spellings[0] after the file of
    // spellings[2].
    let mut x = overlay();
    x.files[1].path = b"/s/a.cc".to_vec();
    x.files[2].path = b"/s/x/".to_vec();
    rejected(&x, "sections.overlay.files[2]");
    let mut x = overlay();
    x.spellings[0].hi = 10;
    x.spellings[2].file = 5;
    rejected(&x, "sections.overlay.spellings[2]");
    // No file before anything else.
    let mut x = overlay();
    x.files.clear();
    rejected(&x, "sections.overlay.files");
}

// spec/edits.md §6: the index of a file by path, and the edits of a file.
#[test]
fn set_file_index_and_edits_of() {
    let x = sample();
    assert_eq!(x.file_index(b"/s/a.cc"), Some(0));
    assert_eq!(x.file_index(b"/s/a.h"), Some(1));
    assert_eq!(x.file_index(b"/s/\xffb.h"), Some(2));
    for missing in [&b"/"[..], b"/s/a", b"/s/a.c", b"/s/b.h", b"/t", b""] {
        assert_eq!(x.file_index(missing), None, "{missing:?}");
    }
    assert_eq!(x.edits_of(0), &x.edits[0..2]);
    assert_eq!(x.edits_of(1), &x.edits[2..3]);
    assert!(x.edits_of(2).is_empty(), "the last file, with no edit");
    assert!(x.edits_of(3).is_empty());
    let y = EditSet { edits: x.edits[2..].to_vec(), groups: Vec::new(), ..sample() };
    assert!(y.edits_of(0).is_empty(), "the first file, with no edit");
    assert_eq!(y.edits_of(1), &y.edits[..]);
    assert!(refused().edits_of(0).is_empty());
}

// spec/edits.md §7.1: find by path, and the obligations of a file.
#[test]
fn overlay_find_and_obligations() {
    let x = overlay();
    assert_eq!(x.find(b"/s/a.cc"), Some(0));
    assert_eq!(x.find(b"/s/a.h"), Some(1));
    assert_eq!(x.find(b"/s/\xffb.h"), Some(2));
    for missing in [&b"/"[..], b"/s/a", b"/s/a.c", b"/s/b.h", b"/t", b""] {
        assert_eq!(x.find(missing), None, "{missing:?}");
    }
    assert_eq!(x.obligations_of(0), 0..2);
    assert_eq!(x.obligations_of(1), 2..2);
    assert_eq!(x.obligations_of(2), 2..3);
    assert_eq!(x.obligations_of(3), 3..3);
    // The last file without obligations, the first with none.
    let mut y = overlay();
    y.spellings = vec![Obligation { file: 1, lo: 0, hi: 1, key: d(0x10) }];
    assert_eq!(y.obligations_of(0), 0..0);
    assert_eq!(y.obligations_of(1), 0..1);
    assert_eq!(y.obligations_of(2), 1..1);
    let mut z = overlay();
    z.spellings.clear();
    assert!(z.obligations_of(0).is_empty());
    assert_eq!(small_overlay().obligations_of(0), 0..1);
}

// ---------------------------------------------------------------------------
// The result's invariants (spec/edits.md §8.5)

fn broken_result(f: impl FnOnce(&mut TransformResult), path: &str) -> String {
    let mut x = result();
    f(&mut x);
    rejected(&x, path)
}

#[test]
fn result_invariants() {
    for scheme in [0, 2] {
        let what = broken_result(|x| x.scheme = scheme, "sections.result.scheme");
        assert_eq!(what, "unknown result scheme");
    }
    let what = broken_result(|x| x.agreement.swap(0, 1), "sections.result.agreement[1]");
    assert_eq!(what, "agreement is not strictly ascending by (rule, observer)");
    broken_result(
        |x| x.agreement[1] = Stale { kept_via: 9, ..x.agreement[0] },
        "sections.result.agreement[1]",
    );
    // Row 15: a stale pair has `direct` or `kept_via` not 0 (verify.md
    // §9.2, WP-111).
    let what = broken_result(
        |x| x.agreement[1] = Stale { rule: 0, observer: d(0xa2), direct: 0, kept_via: 0 },
        "sections.result.agreement[1]",
    );
    assert_eq!(what, "a stale pair has direct and kept_via both 0");
    for (direct, kept_via) in [(1, 0), (0, 1), (2, 5)] {
        let mut x = result();
        x.agreement[1] = Stale { rule: 0, observer: d(0xa2), direct, kept_via };
        x.validate().unwrap();
    }
    let what = broken_result(|x| x.unused = vec![4, 1], "sections.result.unused[1]");
    assert_eq!(what, "unused is not strictly ascending");
    broken_result(|x| x.unused = vec![1, 1], "sections.result.unused[1]");
    let what = broken_result(|x| x.exempt.swap(0, 1), "sections.result.exempt[1]");
    assert_eq!(what, "exempt is not strictly ascending");
    let what =
        broken_result(|x| x.spellings.failed.swap(0, 1), "sections.result.spellings.failed[1]");
    assert_eq!(what, "failed spellings are not strictly ascending by obligation");
    broken_result(|x| x.spellings.failed[1].obligation = 1, "sections.result.spellings.failed[1]");
    let what = broken_result(|x| x.spellings.passed = 2, "sections.result.spellings.obligations");
    assert_eq!(what, "passed and the failed spellings do not add up to obligations");
    broken_result(|x| x.spellings.obligations = 6, "sections.result.spellings.obligations");
    broken_result(|x| x.spellings.passed = u64::MAX, "sections.result.spellings.obligations");
    // The sum does not wrap: u64::MAX + 1 is not 0.
    broken_result(
        |x| {
            x.spellings.passed = u64::MAX;
            x.spellings.failed.truncate(1);
            x.spellings.obligations = 0;
        },
        "sections.result.spellings.obligations",
    );
    let what =
        broken_result(|x| x.residue[2].file = b"s/a.h".to_vec(), "sections.result.residue[2].file");
    assert_eq!(what, "not a canonical absolute path");
    broken_result(|x| x.residue[2].file = b"/s/a\0h".to_vec(), "sections.result.residue[2].file");
    let what = broken_result(
        |x| x.residue.insert(0, Residue { file: b"/".to_vec(), ..x.residue[0].clone() }),
        "sections.result.residue[0].file",
    );
    assert_eq!(what, "the root / is not a file");
    let what = broken_result(|x| x.residue.swap(0, 1), "sections.result.residue[1]");
    assert_eq!(what, "residue is not strictly ascending by (file, lo)");
    // Equal (file, lo), whatever else differs.
    broken_result(
        |x| x.residue[1] = Residue { category: Category::Mention, hi: 9, ..x.residue[0].clone() },
        "sections.result.residue[1]",
    );
    let what = broken_result(|x| x.overlay = None, "sections.result.overlay");
    assert_eq!(what, "overlay is absent for a state other than refused or present for refused");
    let what = broken_result(|x| x.declared = None, "sections.result.declared");
    assert_eq!(what, "declared is absent for a state other than refused or present for refused");
    let refused = |f: fn(&mut TransformResult), path: &str| {
        let mut x = refused_result();
        f(&mut x);
        rejected(&x, path)
    };
    refused(|x| x.overlay = Some(d(7)), "sections.result.overlay");
    refused(|x| x.declared = Some(d(6)), "sections.result.declared");
    let what = refused(|x| x.verdict = Some(RunVerdict::Incomplete), "sections.result.verdict");
    assert_eq!(what, "a refused result has a verdict");
    for (i, name) in TALLY.iter().enumerate().skip(4) {
        let mut x = refused_result();
        let mut t = tally_values(&x.tally);
        t[i] = 1;
        x.tally = Tally {
            selected: t[0],
            unparsed: t[1],
            failed_runs: t[2],
            error_reports: t[3],
            verified: t[4],
            unchanged: t[5],
            mismatch: t[6],
            not_parsed: t[7],
            unverifiable: t[8],
            never_parsed: t[9],
            failed: t[10],
        };
        let what = rejected(&x, &format!("sections.result.tally.{name}"));
        assert_eq!(what, "a refused result has a count of observers verified");
    }
    // Not checked: a residue's range against its file (that needs the
    // file), and whether a verified_with_residue has a reason (a refusal
    // is in the edit set).
    let mut x = result_in(State::VerifiedWithResidue);
    x.residue.retain(|r| r.category == Category::Mention);
    x.residue[0].hi = 1_000_000;
    x.tally.failed_runs = 0;
    x.tally.error_reports = 0;
    x.validate().unwrap();
    let mut x = refused_result();
    x.residue = result().residue;
    x.agreement = result().agreement;
    x.validate().unwrap();
}

// spec/edits.md §8.5 row 6a (spec/landed/verify/WP-070.md §1): what `why`
// decides of a failed spelling, each field alone, for each kind.
#[test]
fn result_failed_spelling_against_why() {
    const OBS: &str = "observers is not 0 for unseen or is 0 for denotes_other";
    const FIRST: &str = "first is present for unseen or absent for denotes_other";
    const FOUND: &str = "found is present for unseen";
    let p = |i: usize, f: &str| format!("sections.result.spellings.failed[{i}].{f}");
    // [0] is denotes_other, [1] unseen.
    assert_eq!(broken_result(|x| x.spellings.failed[0].observers = 0, &p(0, "observers")), OBS);
    assert_eq!(broken_result(|x| x.spellings.failed[1].observers = 1, &p(1, "observers")), OBS);
    assert_eq!(broken_result(|x| x.spellings.failed[0].first = None, &p(0, "first")), FIRST);
    assert_eq!(broken_result(|x| x.spellings.failed[1].first = Some(d(1)), &p(1, "first")), FIRST);
    assert_eq!(broken_result(|x| x.spellings.failed[1].found = Some(d(1)), &p(1, "found")), FOUND);
    // A denotes_other may have no found key (the origin has none); any
    // name is read, for either kind.
    let mut x = result();
    x.spellings.failed[0].found = None;
    x.spellings.failed[0].name = String::new();
    x.spellings.failed[1].name = String::new();
    x.validate().unwrap();
    // In field order within an entry: observers, first, found.
    broken_result(
        |x| {
            x.spellings.failed[1].observers = 1;
            x.spellings.failed[1].first = Some(d(1));
            x.spellings.failed[1].found = Some(d(1));
        },
        &p(1, "observers"),
    );
    broken_result(
        |x| {
            x.spellings.failed[1].first = Some(d(1));
            x.spellings.failed[1].found = Some(d(1));
        },
        &p(1, "first"),
    );
}

// spec/edits.md §8.4, §8.5 row 8a: a residue entry is not empty, its line
// and column 1-based.
#[test]
fn result_residue_range_line_column() {
    let p = |i: usize, f: &str| format!("sections.result.residue[{i}].{f}");
    let what = broken_result(|x| x.residue[2].hi = x.residue[2].lo, &p(2, "lo"));
    assert_eq!(what, "a residue's range is empty");
    broken_result(|x| x.residue[2].hi = 0, &p(2, "lo"));
    let what = broken_result(|x| x.residue[1].line = 0, &p(1, "line"));
    assert_eq!(what, "a residue's line is 0: lines are 1-based");
    let what = broken_result(|x| x.residue[3].column = 0, &p(3, "column"));
    assert_eq!(what, "a residue's column is 0: columns are 1-based");
    // Within an entry: range, line, column; across entries, in order.
    broken_result(
        |x| {
            x.residue[0].line = 0;
            x.residue[0].column = 0;
        },
        &p(0, "line"),
    );
    broken_result(
        |x| {
            x.residue[1].column = 0;
            x.residue[2].hi = 0;
        },
        &p(1, "column"),
    );
    // Line 1, column 1 is the least.
    let mut x = result();
    x.residue[0].line = 1;
    x.residue[0].column = 1;
    x.validate().unwrap();
}

// spec/edits.md §8.5 rows 13 and 14: the tally adds up.
#[test]
fn result_tally_adds_up() {
    const SUM: &str = "the states of the tally do not add up to selected less unparsed";
    // result(): selected 12, unparsed 1, the seven states 11.
    for i in 4..11 {
        let mut x = result();
        let mut t = tally_values(&x.tally);
        t[i] += 1;
        x.tally = tally_from(t);
        assert_eq!(rejected(&x, "sections.result.tally.verified"), SUM, "{}", TALLY[i]);
        let mut x = result();
        let mut t = tally_values(&x.tally);
        t[i] -= 1;
        x.tally = tally_from(t);
        assert_eq!(rejected(&x, "sections.result.tally.verified"), SUM, "{}", TALLY[i]);
    }
    // Selected and unparsed move the target.
    broken_result(|x| x.tally.selected = 13, "sections.result.tally.verified");
    broken_result(|x| x.tally.unparsed = 0, "sections.result.tally.verified");
    let mut x = result();
    x.tally.selected = 13;
    x.tally.unparsed = 2;
    x.validate().unwrap();
    // failed_runs and error_reports are not states.
    let mut x = result();
    x.tally.failed_runs = 100;
    x.tally.error_reports = 100;
    x.validate().unwrap();
    // unparsed above selected, also for a refused result.
    let what = broken_result(|x| x.tally.unparsed = 13, "sections.result.tally.unparsed");
    assert_eq!(what, "unparsed is more than selected");
    let mut x = refused_result();
    x.tally.unparsed = x.tally.selected + 1;
    rejected(&x, "sections.result.tally.unparsed");
    // A sum that would overflow does not wrap.
    broken_result(
        |x| {
            x.tally.verified = u64::MAX;
            x.tally.failed = 2;
        },
        "sections.result.tally.verified",
    );
    // At the boundary: a sum of u64::MAX + 1 is refused, not saturated to
    // a selected of u64::MAX; a sum of u64::MAX is accepted.
    let mut x = result();
    x.tally = Tally { selected: u64::MAX, verified: u64::MAX, failed: 1, ..Tally::default() };
    rejected(&x, "sections.result.tally.verified");
    x.tally.verified = u64::MAX - 1;
    x.validate().unwrap();
    // No verdict (the verify phase did not finish): no state is counted.
    let what = broken_result(|x| x.verdict = None, "sections.result.tally.verified");
    assert_eq!(what, "a result without a verdict has a count of observers verified");
    for (i, name) in TALLY.iter().enumerate().skip(4) {
        let mut x = result();
        x.verdict = None;
        let mut t = tally_values(&x.tally);
        t[4..].fill(0);
        t[i] = 1;
        x.tally = tally_from(t);
        let what = rejected(&x, &format!("sections.result.tally.{name}"));
        assert_eq!(what, "a result without a verdict has a count of observers verified");
    }
    let mut x = result();
    x.verdict = None;
    x.state = State::Incomplete;
    x.tally = Tally { selected: 12, unparsed: 1, failed_runs: 2, ..Tally::default() };
    x.validate().unwrap();
    // A run over no observer: 0 of 0.
    let mut x = result();
    x.tally = Tally { selected: 3, unparsed: 3, ..Tally::default() };
    x.validate().unwrap();
}

// Rows 15 to 17 (spec/landed/verify/WP-070.md §1): what the file alone
// decides of the verdict and the state, rows 2 to 5 of §8.5.
#[test]
fn result_state_against_verdict() {
    use RunVerdict::{Incomplete, NotVerified, Verified};
    const STATE: &str = "the state is not the one the verdict and the tally give";
    const REASON: &str = "a verified result has residue, a failed run or an error";
    let at = "sections.result.state";
    // Every state with every verdict and with none, nothing else barring
    // it: exactly the pairs of rows 2 to 5 validate.
    for s in State::ALL.into_iter().filter(|&s| s != State::Refused) {
        for v in [None, Some(Verified), Some(NotVerified), Some(Incomplete)] {
            let mut x = result_in(s);
            x.verdict = v;
            match v {
                None => {
                    x.tally = Tally { selected: 12, unparsed: 1, ..Tally::default() };
                }
                Some(Verified) => {
                    x.agreement.clear();
                    x.spellings = Spellings { obligations: 5, passed: 5, failed: Vec::new() };
                    x.allow_unparsed = true;
                }
                Some(NotVerified | Incomplete) => {}
            }
            let fits = match s {
                State::Verified | State::VerifiedWithResidue => v == Some(Verified),
                State::NotVerified => v == Some(NotVerified),
                State::Incomplete => v.is_none() || v == Some(Incomplete),
                State::Refused => unreachable!(),
            };
            if fits {
                x.validate().unwrap_or_else(|e| panic!("{s:?} {v:?}: {e}"));
            } else {
                assert_eq!(rejected(&x, at), STATE, "{s:?} {v:?}");
            }
        }
    }
    // Row 16: a verified verdict has no stale pair, no failed spelling.
    let mut x = result_in(State::Incomplete);
    x.verdict = Some(Verified);
    x.spellings = Spellings { obligations: 5, passed: 5, failed: Vec::new() };
    let what = rejected(&x, "sections.result.agreement[0]");
    assert_eq!(what, "a verified verdict has a stale pair");
    let mut x = result_in(State::Verified);
    x.spellings = result().spellings;
    let what = rejected(&x, "sections.result.spellings.failed[0]");
    assert_eq!(what, "a verified verdict has a failed spelling");
    // Row 17: unparsed observers without --allow-unparsed bar both
    // verified states and give incomplete. (An unchecked namespace rule
    // barred them too until WP-111 removed `unchecked`.)
    for s in [State::Verified, State::VerifiedWithResidue, State::Incomplete] {
        let mut x = result_in(State::Verified);
        x.state = s;
        x.allow_unparsed = false;
        if s == State::Incomplete {
            x.validate().unwrap();
        } else {
            assert_eq!(rejected(&x, at), STATE, "{s:?}");
        }
        // With nothing barring: incomplete contradicts a verified verdict.
        let mut x = result_in(State::Verified);
        x.state = s;
        if s == State::Incomplete {
            assert_eq!(rejected(&x, at), STATE);
        } else {
            x.validate().unwrap();
        }
    }
    // unparsed 0 does not bar without --allow-unparsed.
    let mut x = result_in(State::Verified);
    x.allow_unparsed = false;
    x.tally.selected = 11;
    x.tally.unparsed = 0;
    x.validate().unwrap();
    // A verified result has none of the reasons the file shows; a
    // verified_with_residue may.
    for f in [
        (|x: &mut TransformResult| x.residue = result().residue) as fn(&mut TransformResult),
        |x| x.tally.failed_runs = 1,
        |x| x.tally.error_reports = 1,
    ] {
        let mut x = result_in(State::Verified);
        f(&mut x);
        assert_eq!(rejected(&x, at), REASON);
        x.state = State::VerifiedWithResidue;
        x.validate().unwrap();
    }
}

fn tally_from(t: [u64; 11]) -> Tally {
    Tally {
        selected: t[0],
        unparsed: t[1],
        failed_runs: t[2],
        error_reports: t[3],
        verified: t[4],
        unchanged: t[5],
        mismatch: t[6],
        not_parsed: t[7],
        unverifiable: t[8],
        never_parsed: t[9],
        failed: t[10],
    }
}

#[test]
fn result_order_of_checks() {
    let rows: Vec<Break<TransformResult>> = vec![
        (|x| x.scheme = 2, "sections.result.scheme"),
        (|x| x.agreement.swap(0, 1), "sections.result.agreement[1]"),
        (|x| x.unused = vec![4, 1], "sections.result.unused[1]"),
        (|x| x.exempt.swap(0, 1), "sections.result.exempt[1]"),
        (|x| x.spellings.failed.swap(0, 1), "sections.result.spellings.failed[1]"),
        (|x| x.spellings.passed = 2, "sections.result.spellings.obligations"),
        (|x| x.spellings.failed[0].first = None, "sections.result.spellings.failed[0].first"),
        (|x| x.residue[3].file = b"/s/z/".to_vec(), "sections.result.residue[3].file"),
        (|x| x.residue.swap(0, 1), "sections.result.residue[1]"),
        (|x| x.residue[2].line = 0, "sections.result.residue[2].line"),
        (|x| x.state = State::Refused, "sections.result.overlay"),
        (|x| x.declared = None, "sections.result.declared"),
        (|x| x.tally.verified += 1, "sections.result.tally.verified"),
        (|x| x.agreement[0].direct = 0, "sections.result.agreement[0]"),
        (|x| x.state = State::Incomplete, "sections.result.state"),
    ];
    for (i, (f, path)) in rows.iter().enumerate() {
        broken_result(f, path);
        for (g, later) in &rows[i + 1..] {
            let mut x = result();
            g(&mut x);
            f(&mut x);
            let v = x.validate().expect_err("two breaks validate");
            assert!(v.starts_with(&format!("{path}: ")), "{path} before {later}: {v}");
            rejected(&x, path);
        }
    }
    // Refused: verdict before the tally; the first nonzero tally field.
    let mut x = refused_result();
    x.verdict = Some(RunVerdict::Verified);
    x.tally.failed = 1;
    rejected(&x, "sections.result.verdict");
    let mut x = refused_result();
    x.tally.failed = 1;
    x.tally.mismatch = 1;
    rejected(&x, "sections.result.tally.mismatch");
    // overlay before declared, then the verdict.
    let mut x = refused_result();
    x.verdict = Some(RunVerdict::Verified);
    x.declared = Some(d(1));
    rejected(&x, "sections.result.declared");
}

// spec/edits.md §10.6: no invariants beyond the encodings; the names the
// store reads, from a valid record only.
#[test]
fn transform_run_names_of_a_record() {
    let mut x = run();
    x.steps.clear();
    x.validate().unwrap();
    round_trip(&x);
    assert_eq!(transform_run_names(&small_run_bytes()).unwrap(), (d(0x11), 2, 128));
    assert_eq!(
        transform_run_names(&run().to_bytes()).unwrap(),
        (d(0x01), 2, 1_700_000_000_000_000_000)
    );
    // A record cut after started_unix_ns, one with a byte after steps, one
    // with an unknown critical section, one of another kind.
    let bytes = small_run_bytes();
    let start = payload_at(&bytes, 1);
    let payload = &bytes[start..bytes.len() - 33];
    let file = |payload: &[u8], extra: Option<u64>| {
        let mut w = Writer::new(*b"RUNT", 1);
        w.section(1, payload);
        if let Some(id) = extra {
            w.section(id, b"x");
        }
        w.finish()
    };
    let cut = &payload[..payload.len() - 7];
    assert_eq!(transform_run_names(&file(cut, None)).unwrap_err().kind, ErrorKind::Truncated);
    let mut long = payload.to_vec();
    long.push(0);
    assert_eq!(error(transform_run_names(&file(&long, None))).0, ErrorKind::Malformed);
    assert_eq!(error(transform_run_names(&file(payload, Some(2)))).0, ErrorKind::UnknownSection);
    assert_eq!(transform_run_names(&file(payload, Some(64))).unwrap(), (d(0x11), 2, 128));
    assert!(transform_run_names(&small_result_bytes()).is_err());
}

// ---------------------------------------------------------------------------
// Geometry (spec/edits.md §5.4, spec/landed/edits/WP-061.md §1)

#[test]
fn geometry_cases() {
    // (a, b, share_byte, within(a, b), within(b, a), cross)
    type Case = ((u64, u64), (u64, u64), bool, bool, bool, bool);
    let cases: [Case; 12] = [
        ((0, 2), (5, 7), false, false, false, false), // disjoint
        ((0, 2), (2, 4), false, false, false, false), // touching at one end
        ((1, 3), (1, 3), true, true, true, false),    // equal
        ((1, 3), (1, 5), true, true, false, false),   // nested at the start
        ((3, 5), (1, 5), true, true, false, false),   // nested at the end
        ((2, 3), (1, 5), true, true, false, false),   // strictly inside
        ((1, 4), (3, 6), true, false, false, true),   // crossing
        ((1, 4), (0, 2), true, false, false, true),   // crossing, the other way
        ((2, 2), (1, 5), false, false, false, false), // empty, inside
        ((2, 2), (2, 2), false, false, false, false), // empty, equal
        ((4, 2), (1, 5), false, false, false, false), // reversed
        ((0, u64::MAX), (u64::MAX - 1, u64::MAX), true, false, true, false),
    ];
    for (a, b, share, ab, ba, crosses) in cases {
        assert_eq!(share_byte(a, b), share, "share_byte {a:?} {b:?}");
        assert_eq!(share_byte(b, a), share, "share_byte {b:?} {a:?}");
        assert_eq!(within(a, b), ab, "within {a:?} {b:?}");
        assert_eq!(within(b, a), ba, "within {b:?} {a:?}");
        assert_eq!(properly_inside(a, b), ab && a != b, "properly_inside {a:?} {b:?}");
        assert_eq!(properly_inside(b, a), ba && a != b, "properly_inside {b:?} {a:?}");
        assert_eq!(cross(a, b), crosses, "cross {a:?} {b:?}");
        assert_eq!(cross(b, a), crosses, "cross {b:?} {a:?}");
    }
    assert!(!properly_inside((1, 3), (1, 3)));
    assert!(properly_inside((1, 3), (1, 4)));
}

// ---------------------------------------------------------------------------
// basis_id (spec/edits.md §6, spec/model.md §3)

#[test]
fn basis_id_encoding() {
    let a = Basis { observer: d(0xa0), shard: Some(d(0x51)) };
    let b = Basis { observer: d(0xa1), shard: None };
    // H("slip.basis.v1", list<Basis>): count, then observer and opt shard.
    let encoded = hex(&["02", &rep("a0", 32), "01", &rep("51", 32), &rep("a1", 32), "00"]);
    assert_eq!(basis_id(&[a, b]), tagged_digest("slip.basis.v1", &encoded));
    assert_eq!(basis_id(&[]), tagged_digest("slip.basis.v1", &[0]));
    // The order, an absent shard and a shard's id each change it.
    assert_ne!(basis_id(&[a, b]), basis_id(&[b, a]));
    assert_ne!(basis_id(&[a]), basis_id(&[Basis { shard: None, ..a }]));
    assert_ne!(basis_id(&[a]), basis_id(&[Basis { shard: Some(d(0x52)), ..a }]));
    assert_ne!(basis_id(&[a]), basis_id(&[Basis { observer: d(0xa2), ..a }]));
    assert_ne!(basis_id(&[a]), basis_id(&[a, a]));
    // Not the digest of another tag.
    assert_ne!(basis_id(&[a, b]), tagged_digest("slip.observer.v1", &encoded));
}
