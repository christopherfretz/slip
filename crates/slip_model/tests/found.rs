//! The shard's `found` section (section 31, `spec/model.md` §8.8; work
//! package WP-135): its bytes, written out by hand, its JSON form, its
//! invariants F-1 to F-10 and the front-coding's own rules, each at the
//! offset and JSON path §8.8 gives.

use slip_model::container::Writer;
use slip_model::formats::{JsonFormError, dump, undump};
use slip_model::json;
use slip_model::wire::{Encoder, ErrorKind};
use slip_model::{Counter, Digest, FileRef, Found, FoundPlace, Outcome, Phase, Shard, Status};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

/// A map shard with three inputs and `found`.
fn map_shard(found: Option<Found>) -> Shard {
    let input = |p: &[u8], n: u8| FileRef { path: p.to_vec(), size: 9, content: d(n) };
    Shard {
        observer: d(0x30),
        phase: Phase::Map,
        round: 0,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: Some(vec![input(b"/s/a.cc", 1), input(b"/s/b.h", 2), input(b"/s/c.h", 2)]),
        counters: vec![Counter { name: "pp.tokens".to_owned(), value: 5 }],
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found,
        rebuilt_use: None,
    }
}

/// The shard's sections other than `found`, by an encoder of the test's
/// own (`spec/model.md` §8), then `payload` as section 31.
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
    w.section(31, payload);
    w.finish()
}

/// Where section 31's payload begins in `bytes`.
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
        assert_ne!(id, 0, "no section 31");
        let len = uv(bytes, &mut at) as usize;
        if id == 31 {
            return at;
        }
        at += len;
    }
}

fn place(path: &[u8], input: u64) -> FoundPlace {
    FoundPlace { path: path.to_vec(), input }
}

/// `/b/inc/a.h` → 0, `/b/inc/b.h` → 2, `/b/x/\xff.h` → 1; inputs 0 and 2
/// one file.
fn found() -> Found {
    Found {
        places: vec![place(b"/b/inc/a.h", 0), place(b"/b/inc/b.h", 2), place(b"/b/x/\xff.h", 1)],
        classes: vec![vec![0, 2]],
    }
}

/// The payload of [`found`], by hand: places count 3; (0, "/b/inc/a.h", 0);
/// (7, "b.h", 2), the 7 bytes of `/b/inc/`; (3, "x/\xff.h", 1), the 3 of
/// `/b/`; classes count 1: one class of 2, inputs 0 and 2.
const PAYLOAD: &[u8] = b"\x03\
\x00\x0a/b/inc/a.h\x00\
\x07\x03b.h\x02\
\x03\x05x/\xff.h\x01\
\x01\x02\x00\x02";

// §8.8: the bytes, the JSON form (whole paths, non-UTF-8 as JSON bytes are
// written), and binary → JSON → binary.
#[test]
fn found_bytes_and_round_trip() {
    let s = map_shard(Some(found()));
    s.validate().unwrap();
    let bytes = s.to_bytes();
    assert_eq!(bytes, sealed_with(&s, PAYLOAD), "to_bytes agrees with the bytes by hand");
    let back = Shard::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
    let text = json::write_pretty(&s.to_json()).unwrap();
    assert!(text.contains("\"found\""), "{text}");
    assert!(text.contains("\"/b/inc/b.h\""), "{text}");
    assert!(text.contains("\"classes\""), "{text}");
    let from_text = Shard::from_json(&json::parse(text.as_bytes()).unwrap()).unwrap();
    assert_eq!(from_text, s);
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&json::parse(text.as_bytes()).unwrap()).unwrap(), bytes);
}

// §8.8: an empty section (nothing found, every input a file of its own) is
// a section; no section is `None`. They are two shards.
#[test]
fn found_empty_and_absent() {
    let empty = map_shard(Some(Found::default()));
    let none = map_shard(None);
    assert_ne!(empty.to_bytes(), none.to_bytes());
    assert_eq!(Shard::from_bytes(&empty.to_bytes()).unwrap(), empty);
    assert_eq!(Shard::from_bytes(&none.to_bytes()).unwrap(), none);
}

/// The payload of `f` (as the format writes it).
fn payload(f: &Found) -> Vec<u8> {
    let mut e = Encoder::new();
    e.count(f.places.len());
    let mut prev: &[u8] = &[];
    for p in &f.places {
        let shared = prev.iter().zip(&p.path).take_while(|(a, b)| a == b).count();
        e.uv(shared as u64).bytes(&p.path[shared..]).uv(p.input);
        prev = &p.path;
    }
    e.count(f.classes.len());
    for c in &f.classes {
        e.count(c.len());
        for &x in c {
            e.uv(x);
        }
    }
    e.into_bytes()
}

/// `s` is refused by every reader with `what` at `path`; the binary reader
/// at `offset` within section 31's payload.
fn refused(s: &Shard, path: &str, what: &str, offset: usize) {
    let bytes = sealed_with(s, &payload(s.found.as_ref().unwrap()));
    let e = Shard::from_bytes(&bytes).expect_err("from_bytes accepted an invalid shard");
    assert_eq!((e.kind, e.what), (ErrorKind::Malformed, what), "{e}");
    assert_eq!(e.offset, payload_at(&bytes) + offset, "offset");
    assert_eq!(s.validate().unwrap_err(), format!("{path}: {what}"));
    let j = Shard::from_json(&s.to_json()).unwrap_err();
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: what.to_owned() });
}

fn with(f: impl FnOnce(&mut Found)) -> Shard {
    let mut x = found();
    f(&mut x);
    map_shard(Some(x))
}

// F-1: only in a map shard.
#[test]
fn found_f1_phase_map() {
    let mut s = map_shard(Some(found()));
    s.phase = Phase::Verify;
    refused(&s, "sections.found", "a found section in a shard whose phase is not map", 0);
}

// F-2: only with `inputs` (a `failed` shard has none).
#[test]
fn found_f2_with_inputs() {
    let mut s = map_shard(Some(Found::default()));
    s.outcome.status = Status::Failed;
    s.outcome.message = "killed".to_owned();
    s.inputs = None;
    s.counters.clear();
    refused(&s, "sections.found", "a found section in a shard without inputs", 0);
}

// F-3: absolute paths. The second place begins after the count (1 byte)
// and the first place (1 + 1 + 10 + 1 bytes).
#[test]
fn found_f3_absolute() {
    let s = with(|f| f.places[1].path = b"b/inc/b.h".to_vec());
    refused(&s, "sections.found.places[1]", "a found place is not an absolute path", 14);
}

// F-4: strictly ascending; a repeated path is refused as well.
#[test]
fn found_f4_ascending() {
    let s = with(|f| f.places.swap(0, 1));
    refused(&s, "sections.found.places[1]", "found places are not strictly ascending", 14);
    let s = with(|f| f.places[1].path = b"/b/inc/a.h".to_vec());
    refused(&s, "sections.found.places[1]", "found places are not strictly ascending", 14);
}

// F-5: a place's input is one of the shard's inputs; at the `input` field
// (the second place's: 14 + 1 + 1 + 3).
#[test]
fn found_f5_input() {
    let s = with(|f| f.places[1].input = 3);
    refused(&s, "sections.found.places[1].input", "a found place names no input", 19);
}

// F-6 to F-10: the classes. The classes' count is at 28 (after the three
// places: 1 + 13 + 6 + 8), the first class at 29, its inputs at 30, 31.
#[test]
fn found_f6_two_or_more() {
    let s = with(|f| f.classes = vec![vec![1]]);
    refused(&s, "sections.found.classes[0]", "a class of fewer than two inputs", 29);
    let s = with(|f| f.classes = vec![vec![]]);
    refused(&s, "sections.found.classes[0]", "a class of fewer than two inputs", 29);
}

#[test]
fn found_f7_classes_ascending() {
    let mut s =
        map_shard(Some(Found { places: Vec::new(), classes: vec![vec![2, 3], vec![0, 1]] }));
    let mut inputs = s.inputs.take().unwrap();
    inputs.push(FileRef { path: b"/s/d.h".to_vec(), size: 9, content: d(2) });
    s.inputs = Some(inputs);
    // places count 1 byte, classes count 1, first class 3 bytes: at 5.
    refused(&s, "sections.found.classes[1]", "classes are not ascending by their first input", 5);
}

#[test]
fn found_f8_member_is_an_input() {
    let s = with(|f| f.classes = vec![vec![0, 3]]);
    refused(&s, "sections.found.classes[0][1]", "a class names no input", 31);
}

#[test]
fn found_f9_members_ascending() {
    let s = with(|f| f.classes = vec![vec![2, 0]]);
    refused(&s, "sections.found.classes[0][1]", "a class is not strictly ascending", 31);
    let s = with(|f| f.classes = vec![vec![2, 2]]);
    refused(&s, "sections.found.classes[0][1]", "a class is not strictly ascending", 31);
}

#[test]
fn found_f10_one_class_each() {
    let s = with(|f| f.classes = vec![vec![0, 1], vec![1, 2]]);
    // the second class at 32 (29 + 3), its first input at 33.
    refused(&s, "sections.found.classes[1][0]", "an input in two classes", 33);
}

// The front-coding's own rules, at the place's `shared` field, as in
// `negatives`.
#[test]
fn found_encoding_rules() {
    let s = map_shard(None);
    let long = b"\x02\x00\x03/ab\x00\x04\x01c\x00\x00";
    let bytes = sealed_with(&s, long);
    let e = Shard::from_bytes(&bytes).unwrap_err();
    assert_eq!(
        (e.kind, e.what),
        (ErrorKind::Malformed, "a shared prefix longer than the path before")
    );
    assert_eq!(e.offset, payload_at(&bytes) + 7);
    let short = b"\x02\x00\x03/ab\x00\x01\x02ac\x00\x00";
    let bytes = sealed_with(&s, short);
    let e = Shard::from_bytes(&bytes).unwrap_err();
    assert_eq!((e.kind, e.what), (ErrorKind::Malformed, "a shared prefix that is not the longest"));
    assert_eq!(e.offset, payload_at(&bytes) + 7);
    let good = b"\x02\x00\x03/ab\x00\x02\x01c\x01\x00";
    let back = Shard::from_bytes(&sealed_with(&s, good)).unwrap();
    assert_eq!(
        back.found,
        Some(Found { places: vec![place(b"/ab", 0), place(b"/ac", 1)], classes: vec![] })
    );
}
