//! The shard's `negatives` section (section 28, `spec/model.md` §8.6; work
//! package WP-117): its bytes, written out by hand, its JSON form, its
//! invariants N-1 to N-4 and the encoding's own rules, each at the offset
//! and JSON path §8.6 gives.

use slip_model::container::Writer;
use slip_model::formats::{JsonFormError, dump, undump};
use slip_model::json;
use slip_model::wire::{Encoder, ErrorKind};
use slip_model::{Counter, Digest, FileRef, Outcome, Phase, Shard, Status};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn map_shard(negatives: Option<Vec<Vec<u8>>>) -> Shard {
    Shard {
        observer: d(0x30),
        phase: Phase::Map,
        round: 0,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: Some(vec![FileRef { path: b"/s/a.cc".to_vec(), size: 9, content: d(1) }]),
        counters: vec![Counter { name: "pp.tokens".to_owned(), value: 5 }],
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives,
        found: None,
        rebuilt_use: None,
    }
}

/// The shard's sections other than `negatives`, by an encoder of the
/// test's own (`spec/model.md` §8), then `payload` as section 28.
fn sealed_with(s: &Shard, payload: &[u8]) -> Vec<u8> {
    let mut header = Encoder::new();
    header.digest(&s.observer).uv(s.phase as u64).uv(s.round);
    let mut outcome = Encoder::new();
    outcome
        .uv(s.outcome.status as u64)
        .uv(s.outcome.errors)
        .uv(s.outcome.warnings)
        .str(&s.outcome.message);
    let mut w = Writer::new(*b"SHRD", Shard::VERSION);
    w.section(1, &header.into_bytes());
    w.section(2, &outcome.into_bytes());
    if let Some(inputs) = &s.inputs {
        let mut e = Encoder::new();
        e.count(inputs.len());
        for f in inputs {
            e.bytes(&f.path).uv(f.size).digest(&f.content);
        }
        w.section(3, &e.into_bytes());
    }
    let mut counters = Encoder::new();
    counters.count(s.counters.len());
    for c in &s.counters {
        counters.str(&c.name).uv(c.value);
    }
    w.section(4, &counters.into_bytes());
    w.section(28, payload);
    w.finish()
}

/// Where section 28's payload begins in `bytes`.
fn payload_at(bytes: &[u8]) -> usize {
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
        let id = uv(bytes, &mut at);
        assert_ne!(id, 0, "no section 28");
        let len = uv(bytes, &mut at) as usize;
        if id == 28 {
            return at;
        }
        at += len;
    }
}

/// The section's payload for `/b/inc/a.h`, `/b/inc/b.h`, `/b/x/../y/\xff.h`,
/// written out by hand: count 3; (0, "/b/inc/a.h"); (7, "b.h"), the 7
/// bytes of `/b/inc/`; (3, "x/../y/\xff.h"), the 3 of `/b/`.
const PAYLOAD: &[u8] = b"\x03\
\x00\x0a/b/inc/a.h\
\x07\x03b.h\
\x03\x0ax/../y/\xff.h";

fn paths() -> Vec<Vec<u8>> {
    vec![b"/b/inc/a.h".to_vec(), b"/b/inc/b.h".to_vec(), b"/b/x/../y/\xff.h".to_vec()]
}

// §8.6: the bytes, the JSON form (whole paths, non-UTF-8 as JSON bytes are
// written), and binary → JSON → binary.
#[test]
fn negatives_bytes_and_round_trip() {
    let s = map_shard(Some(paths()));
    s.validate().unwrap();
    let bytes = s.to_bytes();
    assert_eq!(bytes, sealed_with(&s, PAYLOAD), "to_bytes agrees with the bytes by hand");
    let back = Shard::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
    let text = json::write_pretty(&s.to_json()).unwrap();
    assert!(text.contains("\"negatives\""), "{text}");
    assert!(text.contains("\"/b/inc/a.h\""), "{text}");
    let from_text = Shard::from_json(&json::parse(text.as_bytes()).unwrap()).unwrap();
    assert_eq!(from_text, s);
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&json::parse(text.as_bytes()).unwrap()).unwrap(), bytes);
}

// §8.6: an empty list is a section (the parse looked and missed nothing);
// no section is `None`. They are two shards.
#[test]
fn negatives_empty_and_absent() {
    let empty = map_shard(Some(Vec::new()));
    let none = map_shard(None);
    assert_ne!(empty.to_bytes(), none.to_bytes());
    assert_eq!(Shard::from_bytes(&empty.to_bytes()).unwrap(), empty);
    assert_eq!(Shard::from_bytes(&none.to_bytes()).unwrap(), none);
}

/// `s` is refused by every reader with `what` at `path`; the binary reader
/// at `offset` within section 28's payload.
fn refused(s: &Shard, payload: &[u8], path: &str, what: &str, offset: usize) {
    let bytes = sealed_with(s, payload);
    let e = Shard::from_bytes(&bytes).expect_err("from_bytes accepted an invalid shard");
    assert_eq!((e.kind, e.what), (ErrorKind::Malformed, what), "{e}");
    assert_eq!(e.offset, payload_at(&bytes) + offset, "offset");
    assert_eq!(s.validate().unwrap_err(), format!("{path}: {what}"));
    let j = Shard::from_json(&s.to_json()).unwrap_err();
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: what.to_owned() });
}

/// The payload of `paths` front-coded (as the format writes it).
fn payload(paths: &[Vec<u8>]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.count(paths.len());
    let mut prev: &[u8] = &[];
    for p in paths {
        let shared = prev.iter().zip(p).take_while(|(a, b)| a == b).count();
        e.uv(shared as u64).bytes(&p[shared..]);
        prev = p;
    }
    e.into_bytes()
}

// N-1: only in a map shard.
#[test]
fn negatives_n1_phase_map() {
    let mut s = map_shard(Some(paths()));
    s.phase = Phase::Verify;
    refused(
        &s,
        &payload(&paths()),
        "sections.negatives",
        "a negatives section in a shard whose phase is not map",
        0,
    );
}

// N-2: only with `inputs` (a `failed` shard has none).
#[test]
fn negatives_n2_with_inputs() {
    let mut s = map_shard(Some(paths()));
    s.outcome.status = Status::Failed;
    s.outcome.message = "killed".to_owned();
    s.inputs = None;
    s.counters.clear();
    refused(
        &s,
        &payload(&paths()),
        "sections.negatives",
        "a negatives section in a shard without inputs",
        0,
    );
}

// N-3: absolute paths. The entry's offset: after the count (1 byte) and
// the first entry (1 + 1 + 10 bytes).
#[test]
fn negatives_n3_absolute() {
    let p = vec![b"/b/inc/a.h".to_vec(), b"b/inc/b.h".to_vec()];
    let s = map_shard(Some(p.clone()));
    refused(
        &s,
        &payload(&p),
        "sections.negatives[1]",
        "a negative input is not an absolute path",
        13,
    );
}

// N-4: strictly ascending; a repeated path is refused as well.
#[test]
fn negatives_n4_ascending() {
    let p = vec![b"/b/inc/b.h".to_vec(), b"/b/inc/a.h".to_vec()];
    let s = map_shard(Some(p.clone()));
    refused(&s, &payload(&p), "sections.negatives[1]", "negatives are not strictly ascending", 13);
    let p = vec![b"/b/inc/a.h".to_vec(), b"/b/inc/a.h".to_vec()];
    let s = map_shard(Some(p.clone()));
    refused(&s, &payload(&p), "sections.negatives[1]", "negatives are not strictly ascending", 13);
}

// The encoding's own rules, at the entry's `shared` field: no longer than
// the path before; the longest common prefix.
#[test]
fn negatives_encoding_rules() {
    let s = map_shard(None);
    let at = |bytes: &[u8]| payload_at(bytes);
    let long = b"\x02\x00\x03/ab\x04\x01c";
    let bytes = sealed_with(&s, long);
    let e = Shard::from_bytes(&bytes).unwrap_err();
    assert_eq!(
        (e.kind, e.what),
        (ErrorKind::Malformed, "a shared prefix longer than the path before")
    );
    assert_eq!(e.offset, at(&bytes) + 6);
    let short = b"\x02\x00\x03/ab\x01\x02ac";
    let bytes = sealed_with(&s, short);
    let e = Shard::from_bytes(&bytes).unwrap_err();
    assert_eq!((e.kind, e.what), (ErrorKind::Malformed, "a shared prefix that is not the longest"));
    assert_eq!(e.offset, at(&bytes) + 6);
    let good = b"\x02\x00\x03/ab\x02\x01c";
    let back = Shard::from_bytes(&sealed_with(&s, good)).unwrap();
    assert_eq!(back.negatives, Some(vec![b"/ab".to_vec(), b"/ac".to_vec()]));
}
