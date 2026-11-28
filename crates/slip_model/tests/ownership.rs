//! Section 35 (`ownership`) of a verify shard and the content-addressed
//! `OwnershipList` blob format (`spec/model.md` §8, §8.9; work package WP-150
//! stage 2).
//!
//! Covers: round trips, goldens (`ownership-list.bin`, `ownership-list.json`,
//! `shard-ownership.json`), every reader row with exact byte offset and JSON
//! path, version rule, property round-trip test `decode(encode(x)) == x`,
//! and byte-level corruption/truncation resilience.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use slip_model::container::{HEADER_LEN, TRAILER_LEN, Writer};
use slip_model::formats::{JsonFormError, MESSAGE_MAX, content_id, dump, undump};
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::ownership::{FixedAs, FixedEntry, MadeEntry, OwnershipList};
use slip_model::wire::{Encoder, ErrorKind};
use slip_model::{Digest, Outcome, Phase, Shard, Status};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn d2(a: u8, b: u8) -> Digest {
    let mut bytes = [0u8; 32];
    bytes[0] = a;
    bytes[31] = b;
    Digest(bytes)
}

fn parse(text: &str) -> Value {
    json::parse(text.as_bytes()).unwrap()
}

fn golden(name: &str, actual: &[u8]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(
        actual,
        expected.as_slice(),
        "{name} differs from its golden; SLIP_BLESS=1 rewrites"
    );
}

fn verify_shard(ownership: Option<Vec<Digest>>) -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Verify,
        round: 1,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: Vec::new(),
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
        ownership,
    }
}

/// Builds a `SHRD` container directly from raw section payloads (so invalid
/// shards can be encoded and checked at exact file offsets).
fn raw_shard(s: &Shard) -> Vec<u8> {
    let mut header = Encoder::new();
    header.digest(&s.observer).uv(s.phase as u64).uv(s.round);
    let mut outcome = Encoder::new();
    outcome
        .uv(s.outcome.status as u64)
        .uv(s.outcome.errors)
        .uv(s.outcome.warnings)
        .str(&s.outcome.message);
    let mut counters = Encoder::new();
    counters.count(s.counters.len());
    let mut w = Writer::new(Shard::KIND, Shard::VERSION);
    w.section(1, header.as_bytes());
    w.section(2, outcome.as_bytes());
    w.section(4, counters.as_bytes());
    if let Some(lists) = &s.ownership {
        let mut o = Encoder::new();
        o.count(lists.len());
        for id in lists {
            o.digest(id);
        }
        w.section(35, o.as_bytes());
    }
    w.finish()
}

/// File offset of section `id`'s payload in a container `bytes`.
fn payload_at(bytes: &[u8], want: u64) -> usize {
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
    let mut at = HEADER_LEN;
    loop {
        let id = uv(bytes, &mut at);
        assert_ne!(id, 0, "section {want} not found");
        let len = usize::try_from(uv(bytes, &mut at)).unwrap();
        if id == want {
            return at;
        }
        at += len;
    }
}

/// Encodes an `OwnershipList` with explicit version and no validation,
/// returning `(bytes, offsets)` for exact offset assertions.
#[derive(Default)]
struct RawListAt {
    through: usize,
    made_list: usize,
    made: Vec<usize>,
    made_line: Vec<usize>,
    fixed: Vec<usize>,
    fixed_line: Vec<usize>,
    fixed_name: Vec<usize>,
}

fn raw_list(version: u64, list: &OwnershipList) -> (Vec<u8>, RawListAt) {
    let mut e = Encoder::new();
    let mut at = RawListAt::default();
    e.uv(version).bytes(&list.file);
    at.through = e.as_bytes().len();
    e.bytes(&list.through);
    at.made_list = e.as_bytes().len();
    e.count(list.made.len());
    for m in &list.made {
        at.made.push(e.as_bytes().len());
        e.digest(&m.key);
        match &m.post {
            None => e.u8(0),
            Some(p) => e.u8(1).digest(p),
        };
        e.uv(m.rule);
        at.made_line.push(e.as_bytes().len());
        e.uv(m.line).uv(m.column);
    }
    e.count(list.fixed.len());
    for f in &list.fixed {
        at.fixed.push(e.as_bytes().len());
        e.digest(&f.key).uv(f.as_ as u64);
        at.fixed_line.push(e.as_bytes().len());
        e.uv(f.line).uv(f.column);
        at.fixed_name.push(e.as_bytes().len());
        e.str(&f.name);
    }
    (e.into_bytes(), at)
}

fn sample_list() -> OwnershipList {
    OwnershipList {
        file: b"/src/own/a.h".to_vec(),
        through: Vec::new(),
        made: vec![
            MadeEntry { key: d(0x10), post: None, rule: 0, line: 0, column: 0 },
            MadeEntry { key: d(0x10), post: Some(d(0x20)), rule: 2, line: 12, column: 5 },
            MadeEntry { key: d(0x30), post: Some(d(0x40)), rule: 1 << 54, line: 44, column: 19 },
        ],
        fixed: vec![
            FixedEntry {
                key: d(0x50),
                as_: FixedAs::T,
                line: 3,
                column: 21,
                name: "old::hook(int)".to_owned(),
            },
            FixedEntry {
                key: d(0x50),
                as_: FixedAs::M,
                line: 3,
                column: 21,
                name: "old::hook(int)".to_owned(),
            },
            FixedEntry {
                key: d(0x60),
                as_: FixedAs::M,
                line: 0,
                column: 0,
                name: "fresh::g(int)".to_owned(),
            },
        ],
    }
}

fn through_list() -> OwnershipList {
    OwnershipList {
        file: b"/src/own/own.h".to_vec(),
        through: b"/src/dep/ro_inc.h".to_vec(),
        made: Vec::new(),
        fixed: vec![FixedEntry {
            key: d(0x70),
            as_: FixedAs::M,
            line: 1,
            column: 21,
            name: "fresh::g(int)".to_owned(),
        }],
    }
}

// ---------------------------------------------------------------------------
// Section 35 (spec/model.md §8, §8.9)

#[test]
fn section_35_round_trip_and_golden() {
    for lists in [vec![], vec![d(0x11)], vec![d(0x11), d(0x22), d(0xff)]] {
        let s = verify_shard(Some(lists));
        s.validate().unwrap();
        let bytes = s.to_bytes();
        assert_eq!(bytes, raw_shard(&s));
        let back = Shard::from_bytes(&bytes).unwrap();
        assert_eq!(back, s);
        let text = dump(&bytes).unwrap();
        let v = parse(&text);
        assert_eq!(Shard::from_json(&v).unwrap(), s);
        assert_eq!(undump(&v).unwrap(), bytes);
        assert_eq!(content_id(&bytes).unwrap().as_bytes(), &bytes[bytes.len() - TRAILER_LEN..]);
    }
    let s = verify_shard(Some(vec![d(0x11), d(0x22)]));
    let text = dump(&s.to_bytes()).unwrap();
    golden("shard-ownership.json", text.as_bytes());
}

#[test]
fn section_35_invariants() {
    // 1. Phase != verify.
    let mut shard = verify_shard(Some(vec![d(0x11)]));
    shard.phase = Phase::Map;
    let bytes = raw_shard(&shard);
    let payload_off = payload_at(&bytes, 35);
    let err = Shard::from_bytes(&bytes).unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Malformed, payload_off), "{err}");
    assert!(err.what.contains("phase is not verify"), "{err}");
    let json_err = Shard::from_json(&shard.to_json()).unwrap_err();
    assert_eq!(
        json_err,
        JsonFormError { path: "sections.ownership".into(), message: err.what.into() }
    );
    assert!(shard.validate().unwrap_err().contains("sections.ownership:"));

    // 2. Status != ok (diagnosed, rejected, failed).
    for status in [Status::Diagnosed, Status::Rejected, Status::Failed] {
        let mut bad_shard = verify_shard(Some(vec![]));
        bad_shard.outcome = Outcome { status, errors: 1, warnings: 0, message: "err".to_owned() };
        let raw = raw_shard(&bad_shard);
        let off = payload_at(&raw, 35);
        let wire_err = Shard::from_bytes(&raw).unwrap_err();
        assert_eq!((wire_err.kind, wire_err.offset), (ErrorKind::Malformed, off), "{wire_err}");
        assert!(wire_err.what.contains("status is not ok"), "{wire_err}");
        let jerr = Shard::from_json(&bad_shard.to_json()).unwrap_err();
        assert_eq!(
            jerr,
            JsonFormError { path: "sections.ownership".into(), message: wire_err.what.into() }
        );
    }

    // 3. List ids unsorted or duplicated.
    for bad in [vec![d(0x22), d(0x11)], vec![d(0x11), d(0x22), d(0x22)]] {
        let idx = bad.len() - 1;
        let bad_shard = verify_shard(Some(bad));
        let raw = raw_shard(&bad_shard);
        let off = payload_at(&raw, 35);
        let wire_err = Shard::from_bytes(&raw).unwrap_err();
        // 1 byte count + idx * 32 bytes.
        assert_eq!(
            (wire_err.kind, wire_err.offset),
            (ErrorKind::Malformed, off + 1 + idx * 32),
            "{wire_err}"
        );
        assert!(wire_err.what.contains("not strictly ascending"), "{wire_err}");
        let jerr = Shard::from_json(&bad_shard.to_json()).unwrap_err();
        assert_eq!(
            jerr,
            JsonFormError {
                path: format!("sections.ownership[{idx}]"),
                message: wire_err.what.into()
            }
        );
    }

    // Order of checks: phase before status before list ordering.
    let mut ordered = verify_shard(Some(vec![d(0x22), d(0x11)]));
    ordered.phase = Phase::Map;
    ordered.outcome =
        Outcome { status: Status::Diagnosed, errors: 1, warnings: 0, message: "e".into() };
    let wire_err = Shard::from_bytes(&raw_shard(&ordered)).unwrap_err();
    assert!(wire_err.what.contains("phase is not verify"), "{wire_err}");
}

// ---------------------------------------------------------------------------
// OwnershipList blob format (spec/model.md §8.9)

#[test]
fn ownership_list_round_trip_and_goldens() {
    let x = sample_list();
    x.validate().unwrap();
    let bytes = x.to_bytes();
    assert_eq!(bytes, raw_list(OwnershipList::VERSION, &x).0);
    golden("ownership-list.bin", &bytes);
    let back = OwnershipList::from_bytes(&bytes).unwrap();
    assert_eq!(back, x);
    let text = json::write_pretty(&x.to_json()).unwrap();
    golden("ownership-list.json", text.as_bytes());
    let from_json = OwnershipList::from_json(&parse(&text)).unwrap();
    assert_eq!(from_json, x);
    assert_eq!(from_json.to_bytes(), bytes);

    // Also round-trip a list with non-empty `through`, non-UTF-8 file path,
    // and empty file path (no location).
    let t = through_list();
    t.validate().unwrap();
    assert_eq!(OwnershipList::from_bytes(&t.to_bytes()).unwrap(), t);
    assert_eq!(OwnershipList::from_json(&t.to_json()).unwrap(), t);

    let non_utf8 = OwnershipList {
        file: vec![b'/', 0xff, b'a'],
        through: Vec::new(),
        made: vec![MadeEntry { key: d(1), post: Some(d(2)), rule: 0, line: 1, column: 1 }],
        fixed: Vec::new(),
    };
    non_utf8.validate().unwrap();
    assert_eq!(OwnershipList::from_bytes(&non_utf8.to_bytes()).unwrap(), non_utf8);
    assert_eq!(OwnershipList::from_json(&non_utf8.to_json()).unwrap(), non_utf8);

    let no_file = OwnershipList {
        file: Vec::new(),
        through: Vec::new(),
        made: Vec::new(),
        fixed: vec![FixedEntry {
            key: d(3),
            as_: FixedAs::T,
            line: 0,
            column: 0,
            name: "builtin".to_owned(),
        }],
    };
    no_file.validate().unwrap();
    assert_eq!(OwnershipList::from_bytes(&no_file.to_bytes()).unwrap(), no_file);
    assert_eq!(OwnershipList::from_json(&no_file.to_json()).unwrap(), no_file);
}

#[test]
fn ownership_list_version_rule() {
    let list = sample_list();
    for bad_ver in [0u64, 2, 99] {
        let (bytes, _) = raw_list(bad_ver, &list);
        let err = OwnershipList::from_bytes(&bytes).unwrap_err();
        assert_eq!((err.kind, err.offset), (ErrorKind::UnsupportedVersion, 0), "{err}");
        let mut val = list.to_json();
        let Value::Object(members) = &mut val else { unreachable!() };
        members[0].1 = Value::Int(i128::from(bad_ver));
        let jerr = OwnershipList::from_json(&val).unwrap_err();
        assert_eq!(jerr.path, "version");
    }
}

#[test]
fn ownership_list_reader_rows() {
    let check = |list: &OwnershipList, expected_path: &str, expected_off: usize, substr: &str| {
        let (bytes, _) = raw_list(OwnershipList::VERSION, list);
        let e = OwnershipList::from_bytes(&bytes).expect_err("from_bytes accepted invalid list");
        assert_eq!((e.kind, e.offset), (ErrorKind::Malformed, expected_off), "{e}");
        assert!(e.what.contains(substr), "expected `{substr}` in `{}`", e.what);
        let j =
            OwnershipList::from_json(&list.to_json()).expect_err("from_json accepted invalid list");
        assert_eq!(j, JsonFormError { path: expected_path.to_owned(), message: e.what.to_owned() });
        let v_err = list.validate().expect_err("validate accepted invalid list");
        assert!(v_err.contains(substr), "{v_err}");
    };

    // Row 1: empty list.
    let empty = OwnershipList {
        file: b"/s/a.h".to_vec(),
        through: Vec::new(),
        made: Vec::new(),
        fixed: Vec::new(),
    };
    let (_, at) = raw_list(OwnershipList::VERSION, &empty);
    check(&empty, "", at.made_list, "empty ownership list");

    // Row 2: through without a file.
    let mut bad = through_list();
    bad.file.clear();
    let (_, at) = raw_list(OwnershipList::VERSION, &bad);
    check(&bad, "through", at.through, "through without a file");

    // Row 3: made entry in a list with non-empty through.
    let mut bad = through_list();
    bad.made.push(MadeEntry { key: d(1), post: Some(d(2)), rule: 0, line: 1, column: 1 });
    let (_, at) = raw_list(OwnershipList::VERSION, &bad);
    check(&bad, "made[0]", at.made[0], "made entry in a list entered through a read-only file");

    // Row 4: made not strictly ascending by (key, post) — key reversed, post reversed, duplicate.
    for made in [
        vec![
            MadeEntry { key: d(2), post: None, rule: 0, line: 1, column: 1 },
            MadeEntry { key: d(1), post: None, rule: 0, line: 2, column: 1 },
        ],
        vec![
            MadeEntry { key: d(1), post: Some(d(5)), rule: 0, line: 1, column: 1 },
            MadeEntry { key: d(1), post: None, rule: 0, line: 2, column: 1 },
        ],
        vec![
            MadeEntry { key: d(1), post: Some(d(5)), rule: 0, line: 1, column: 1 },
            MadeEntry { key: d(1), post: Some(d(4)), rule: 0, line: 2, column: 1 },
        ],
        vec![
            MadeEntry { key: d(1), post: Some(d(5)), rule: 0, line: 1, column: 1 },
            MadeEntry { key: d(1), post: Some(d(5)), rule: 1, line: 2, column: 1 },
        ],
        vec![
            MadeEntry { key: d(1), post: None, rule: 0, line: 1, column: 1 },
            MadeEntry { key: d(1), post: None, rule: 1, line: 2, column: 1 },
        ],
    ] {
        let bad = OwnershipList { made, ..sample_list() };
        let (_, at) = raw_list(OwnershipList::VERSION, &bad);
        check(&bad, "made[1]", at.made[1], "made entries are not strictly ascending");
    }

    // Row 5: made line/column 0 mismatch.
    for (line, column) in [(0, 1), (1, 0)] {
        let mut bad = sample_list();
        bad.made[1].line = line;
        bad.made[1].column = column;
        let (_, at) = raw_list(OwnershipList::VERSION, &bad);
        check(&bad, "made[1].line", at.made_line[1], "line is 0 and column is not");
    }

    // Row 6: fixed not strictly ascending by (key, as) — key reversed, as reversed, duplicate.
    let f = |k: u8, as_: FixedAs| FixedEntry {
        key: d(k),
        as_,
        line: 1,
        column: 1,
        name: "x".to_owned(),
    };
    for fixed in [
        vec![f(2, FixedAs::T), f(1, FixedAs::T)],
        vec![f(1, FixedAs::M), f(1, FixedAs::T)],
        vec![f(1, FixedAs::T), f(1, FixedAs::T)],
        vec![f(1, FixedAs::M), f(1, FixedAs::M)],
    ] {
        let bad = OwnershipList { fixed, ..sample_list() };
        let (_, at) = raw_list(OwnershipList::VERSION, &bad);
        check(&bad, "fixed[1]", at.fixed[1], "fixed entries are not strictly ascending");
    }

    // Row 7: fixed line/column 0 mismatch.
    for (line, column) in [(0, 5), (5, 0)] {
        let mut bad = sample_list();
        bad.fixed[0].line = line;
        bad.fixed[0].column = column;
        let (_, at) = raw_list(OwnershipList::VERSION, &bad);
        check(&bad, "fixed[0].line", at.fixed_line[0], "line is 0 and column is not");
    }

    // Row 8: fixed name > 4096 bytes.
    let mut bad = sample_list();
    bad.fixed[2].name = "a".repeat(MESSAGE_MAX + 1);
    let (_, at) = raw_list(OwnershipList::VERSION, &bad);
    check(&bad, "fixed[2].name", at.fixed_name[2], "longer than 4096 bytes");
}

#[test]
fn ownership_list_wire_errors() {
    // Trailing byte.
    let mut bytes = sample_list().to_bytes();
    bytes.push(0x00);
    let err = OwnershipList::from_bytes(&bytes).unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Malformed, bytes.len() - 1));

    // Unlisted FixedAs (code 2).
    let mut enc = Encoder::new();
    enc.uv(1).bytes(b"/a").bytes(b"").count(0).count(1).digest(&d(1));
    let as_at = enc.as_bytes().len();
    enc.uv(2).uv(1).uv(1).str("f");
    let err = OwnershipList::from_bytes(enc.as_bytes()).unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Malformed, as_at));

    // Bad opt tag (2) for Made.post.
    let mut enc = Encoder::new();
    enc.uv(1).bytes(b"/a").bytes(b"").count(1).digest(&d(1));
    let tag_at = enc.as_bytes().len();
    enc.u8(2).uv(0).uv(1).uv(1).count(0);
    let err = OwnershipList::from_bytes(enc.as_bytes()).unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Malformed, tag_at));
}

// ---------------------------------------------------------------------------
// Property / fuzz round-trip & corruption resilience (spec/model.md §2, §8.9)

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn byte(&mut self) -> u8 {
        (self.next() & 0xff) as u8
    }

    fn digest(&mut self) -> Digest {
        let a = self.byte();
        let b = self.byte();
        d2(a, b)
    }

    fn path(&mut self) -> Vec<u8> {
        let len = self.below(16) as usize;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            let b = if self.below(10) == 0 {
                0xff // non-UTF-8
            } else {
                b'a' + (self.below(26) as u8)
            };
            out.push(b);
        }
        out
    }

    fn utf8_name(&mut self) -> String {
        let len = self.below(40) as usize;
        let mut out = String::with_capacity(len);
        for _ in 0..len {
            out.push(char::from(b'a' + (self.below(26) as u8)));
        }
        out
    }
}

fn gen_valid_list(r: &mut Lcg) -> OwnershipList {
    let has_through = r.below(5) == 0;
    let file = if has_through {
        let mut p = r.path();
        if p.is_empty() {
            p.push(b'/');
        }
        p
    } else if r.below(8) == 0 {
        Vec::new()
    } else {
        r.path()
    };
    let through = if has_through {
        let mut p = r.path();
        if p.is_empty() {
            p.push(b't');
        }
        p
    } else {
        Vec::new()
    };
    let mut made = Vec::new();
    if !has_through {
        let n = r.below(6);
        for _ in 0..n {
            let post = (r.below(4) != 0).then(|| r.digest());
            let (line, column) =
                if r.below(5) == 0 { (0, 0) } else { (1 + r.below(10_000), 1 + r.below(500)) };
            let rule = if r.below(10) == 0 { (1u64 << 54) + r.below(100) } else { r.below(50) };
            made.push(MadeEntry { key: r.digest(), post, rule, line, column });
        }
        made.sort_by_key(|m| (m.key, m.post, m.rule, m.line, m.column));
        made.dedup_by_key(|m| (m.key, m.post));
    }
    let mut fixed = Vec::new();
    let n = r.below(6) + u64::from(made.is_empty());
    for _ in 0..n {
        let as_ = if r.below(2) == 0 { FixedAs::T } else { FixedAs::M };
        let (line, column) =
            if r.below(5) == 0 { (0, 0) } else { (1 + r.below(10_000), 1 + r.below(500)) };
        fixed.push(FixedEntry { key: r.digest(), as_, line, column, name: r.utf8_name() });
    }
    fixed.sort_by(|a, b| {
        (a.key, a.as_, a.line, a.column, &a.name).cmp(&(b.key, b.as_, b.line, b.column, &b.name))
    });
    fixed.dedup_by_key(|f| (f.key, f.as_));
    OwnershipList { file, through, made, fixed }
}

// Property test: 2000 generated OwnershipList values and section 35 shards
// round-trip through binary and JSON identically.
#[test]
fn property_round_trip() {
    let mut r = Lcg(0x0150_0002);
    for case in 0..2000 {
        let list = gen_valid_list(&mut r);
        list.validate().unwrap_or_else(|e| panic!("case {case}: {e}"));
        let bytes = list.to_bytes();
        let decoded = OwnershipList::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("case {case}: decode failed: {e}"));
        assert_eq!(decoded, list, "case {case}: binary round-trip");
        assert_eq!(decoded.to_bytes(), bytes, "case {case}: canonical binary");

        let json_val = list.to_json();
        let from_json = OwnershipList::from_json(&json_val)
            .unwrap_or_else(|e| panic!("case {case}: json failed: {e}"));
        assert_eq!(from_json, list, "case {case}: json round-trip");
        assert_eq!(from_json.to_bytes(), bytes, "case {case}: json -> binary");

        if case % 5 == 0 {
            let n = r.below(6);
            let mut ids: Vec<Digest> = (0..n).map(|_| r.digest()).collect();
            ids.sort();
            ids.dedup();
            let shard = verify_shard(Some(ids));
            let sb = shard.to_bytes();
            let sb_back = Shard::from_bytes(&sb).unwrap();
            assert_eq!(sb_back, shard, "case {case}: shard section 35");
            assert_eq!(sb_back.to_bytes(), sb);
            assert_eq!(Shard::from_json(&shard.to_json()).unwrap(), shard);
        }
    }
}

// Fuzz/corruption test: every truncation, single-byte flip/overwrite, and
// random byte streams never panic, and any accepted stream re-encodes
// byte-for-byte identically (`spec/model.md` §1 rule 1).
#[test]
fn corrupted_ownership_lists_and_section_35() {
    let check_blob = |bytes: &[u8]| {
        let res = catch_unwind(AssertUnwindSafe(|| OwnershipList::from_bytes(bytes)));
        let Ok(res) = res else { panic!("OwnershipList::from_bytes panicked") };
        if let Ok(list) = res {
            list.validate().unwrap();
            assert_eq!(list.to_bytes(), bytes, "accepted blob must re-encode identically");
            assert_eq!(OwnershipList::from_json(&list.to_json()).unwrap(), list);
        }
    };

    for base in [sample_list().to_bytes(), through_list().to_bytes()] {
        for n in 0..base.len() {
            check_blob(&base[..n]);
        }
        for i in 0..base.len() {
            for v in [0x00, 0x01, 0x02, 0x7f, 0x80, 0xff, base[i] ^ 1, base[i].wrapping_add(1)] {
                if v != base[i] {
                    let mut m = base.clone();
                    m[i] = v;
                    check_blob(&m);
                }
            }
        }
    }

    // Random byte strings.
    let mut r = Lcg(0x0150_dead);
    for _ in 0..2000 {
        let len = r.below(128) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| r.byte()).collect();
        check_blob(&bytes);
    }

    // Corrupted section 35 inside a valid shard container (resealed trailer).
    let base_shard = verify_shard(Some(vec![d(0x11), d(0x22)])).to_bytes();
    let p = payload_at(&base_shard, 35);
    let end = base_shard.len() - TRAILER_LEN - 1;
    for i in p..end {
        for v in [0x00, 0x01, 0x80, 0xff, base_shard[i] ^ 1] {
            let mut m = base_shard.clone();
            m[i] = v;
            let body = m.len() - TRAILER_LEN;
            let t = content_digest(&m[..body]);
            m[body..].copy_from_slice(t.as_bytes());
            let res = catch_unwind(AssertUnwindSafe(|| Shard::from_bytes(&m)));
            let Ok(res) = res else { panic!("Shard::from_bytes panicked") };
            if let Ok(s) = res {
                s.validate().unwrap();
                assert_eq!(s.to_bytes(), m);
                assert_eq!(Shard::from_json(&s.to_json()).unwrap(), s);
            }
        }
    }
}
