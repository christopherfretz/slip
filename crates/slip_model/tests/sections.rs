//! The shard's `digests` section (`spec/digest.md` §11, section 19) and
//! `verdict` section (`spec/verify.md` §6, section 21), in binary and JSON
//! form (`spec/model.md` §2, §8, §10). Work package WP-015; the offsets and
//! paths pinned here are those of `spec/model.md` §8.2 and §8.3. Each test
//! names the section it covers.

use std::fs;
use std::path::Path;

use slip_model::agreement::{Binding, RuleUse, Use};
use slip_model::container::{TRAILER_LEN, Writer};
use slip_model::digests::{
    Digests, GROUP_TAG, Group, GroupClass, SCHEME, TU_TAG, Unit, group_digest, tu_digest,
};
use slip_model::edits::Edits;
use slip_model::formats::{JsonFormError, content_id, dump, undump};
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::spellings::{Failed, Span, Spellings};
use slip_model::verdict::{Finding, FindingKind, Site, Verdict, VerdictKind, Where};
use slip_model::wire::{Encoder, Error, ErrorKind};
use slip_model::{Counter, Digest, FileRef, Outcome, Phase, Shard, Status};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn unit(key: u8, digest: u8, flags: u64) -> Unit {
    Unit { key: d(key), digest: d(digest), flags }
}

/// A group with its unit list, its count and digest computed from it.
fn listed(class: GroupClass, file: &[u8], units: Vec<Unit>) -> Group {
    Group {
        class,
        file: file.to_vec(),
        count: units.len() as u64,
        indeterminate: units.iter().filter(|u| u.is_indeterminate()).count() as u64,
        digest: group_digest(&units),
        units: Some(units),
    }
}

/// A group without its unit list and without indeterminate units.
fn summed(class: GroupClass, file: &[u8], count: u64, digest: Digest) -> Group {
    Group { class, file: file.to_vec(), count, indeterminate: 0, digest, units: None }
}

fn digests_of(groups: Vec<Group>) -> Digests {
    Digests { scheme: SCHEME, tu: tu_digest(&groups), groups }
}

/// Recomputes every listed group's digest and the `tu` digest, so that a
/// test that breaks another rule breaks only that one.
fn resum(s: &mut Shard) {
    let ds = s.digests.as_mut().unwrap();
    for g in &mut ds.groups {
        if let Some(units) = &g.units {
            g.digest = group_digest(units);
        }
    }
    ds.tu = tu_digest(&ds.groups);
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

fn sealed(sections: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut w = Writer::new(*b"SHRD", 1);
    for (id, p) in sections {
        w.section(*id, p);
    }
    w.finish()
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

// ---------------------------------------------------------------------------
// A second, independent encoder of the shard (spec/model.md §8,
// spec/digest.md §11, spec/verify.md §6), used to build files that break an
// invariant (`to_bytes` debug-asserts validity) and to cross-check `to_bytes`.

fn raw_opt_digest(e: &mut Encoder, v: Option<Digest>) {
    match v {
        None => e.u8(0),
        Some(x) => e.u8(1).digest(&x),
    };
}

fn raw_site(e: &mut Encoder, s: &Site) {
    e.bytes(&s.file).uv(s.line).uv(s.column).str(&s.node).str(&s.what);
}

fn raw_sections(s: &Shard) -> Vec<(u64, Vec<u8>)> {
    let mut s1 = Encoder::new();
    s1.digest(&s.observer).uv(s.phase as u64).uv(s.round);
    let mut s2 = Encoder::new();
    s2.uv(s.outcome.status as u64)
        .uv(s.outcome.errors)
        .uv(s.outcome.warnings)
        .str(&s.outcome.message);
    let mut out = vec![(1, s1.into_bytes()), (2, s2.into_bytes())];
    if let Some(inputs) = &s.inputs {
        let mut s3 = Encoder::new();
        s3.count(inputs.len());
        for f in inputs {
            s3.bytes(&f.path).uv(f.size).digest(&f.content);
        }
        out.push((3, s3.into_bytes()));
    }
    let mut s4 = Encoder::new();
    s4.count(s.counters.len());
    for c in &s.counters {
        s4.str(&c.name).uv(c.value);
    }
    out.push((4, s4.into_bytes()));
    if let Some(x) = &s.edits {
        // Only an empty section: `tests/edits.rs` encodes the rest.
        assert!(x.failure.is_none() && x.files.is_empty() && x.claims.is_empty());
        assert!(x.subjects.is_empty() && x.links.is_empty() && x.groups.is_empty());
        assert!(x.reports.is_empty() && x.changed.is_empty() && x.renamed.is_empty());
        let mut e = Encoder::new();
        e.uv(x.scheme).digest(&x.transform).u8(0);
        for _ in 0..8 {
            e.count(0);
        }
        out.push((17, e.into_bytes()));
    }
    if let Some(ds) = &s.digests {
        let mut e = Encoder::new();
        e.uv(ds.scheme).digest(&ds.tu).count(ds.groups.len());
        for g in &ds.groups {
            e.uv(g.class as u64).bytes(&g.file).uv(g.count).uv(g.indeterminate).digest(&g.digest);
            match &g.units {
                None => {
                    e.u8(0);
                }
                Some(units) => {
                    e.u8(1).count(units.len());
                    for u in units {
                        e.digest(&u.key).digest(&u.digest).uv(u.flags);
                    }
                }
            }
        }
        out.push((19, e.into_bytes()));
    }
    if let Some(v) = &s.verdict {
        let mut e = Encoder::new();
        e.uv(v.kind as u64)
            .digest(&v.pre)
            .digest(&v.rules)
            .str(&v.reason)
            .uv(v.only_pre)
            .uv(v.only_post)
            .count(v.used.len());
        for k in &v.used {
            e.digest(k);
        }
        e.uv(v.total).count(v.findings.len());
        for f in &v.findings {
            e.uv(f.kind as u64);
            raw_opt_digest(&mut e, f.key);
            e.bytes(&f.file);
            raw_opt_digest(&mut e, f.pre);
            raw_opt_digest(&mut e, f.post);
            e.str(&f.name);
            match &f.location {
                None => {
                    e.u8(0);
                }
                Some(w) => {
                    e.u8(1);
                    raw_site(&mut e, &w.pre);
                    raw_site(&mut e, &w.post);
                }
            }
        }
        out.push((21, e.into_bytes()));
    }
    if let Some(b) = &s.binding {
        let mut e = Encoder::new();
        raw_opt_digest(&mut e, b.overlay);
        e.digest(&b.declared).uv(b.pre);
        out.push((24, e.into_bytes()));
    }
    if let Some(r) = &s.rule_use {
        out.push((25, raw_rule_use(r).0));
    }
    if let Some(x) = &s.spellings {
        out.push((26, raw_spellings(x).0));
    }
    out
}

/// The payload of a `rule_use` section, and the offset in it of each use.
fn raw_rule_use(r: &RuleUse) -> (Vec<u8>, Vec<usize>) {
    let mut e = Encoder::new();
    e.digest(&r.rules).count(r.uses.len());
    let mut at = Vec::new();
    for u in &r.uses {
        at.push(e.as_bytes().len());
        e.uv(u.rule).uv(u.via).uv(u.direct);
    }
    (e.into_bytes(), at)
}

/// The payload of a spellings section, and the offsets in it of each
/// span, each span's count and each failed entry.
fn raw_spellings(x: &Spellings) -> (Vec<u8>, Vec<usize>, Vec<usize>, Vec<usize>) {
    let mut e = Encoder::new();
    e.uv(x.seen).count(x.held.len());
    let (mut held, mut counts, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for h in &x.held {
        held.push(e.as_bytes().len());
        e.uv(h.first);
        counts.push(e.as_bytes().len());
        e.uv(h.count);
    }
    e.count(x.failed.len());
    for f in &x.failed {
        failed.push(e.as_bytes().len());
        e.uv(f.index);
        raw_opt_digest(&mut e, f.found);
        e.str(&f.name);
    }
    (e.into_bytes(), held, counts, failed)
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

fn raw(s: &Shard) -> Vec<u8> {
    sealed(&raw_sections(s))
}

// ---------------------------------------------------------------------------
// Two shards whose bytes are written out by hand

/// A map shard: observer 33…, round 0, ok; counters [digest.units = 3];
/// digests with three groups: declared "/a" with units
/// [(41…, 42…, 0), (43…, 44…, 1)] (count 2, indeterminate 1), derived ""
/// (builtins) count 1 indeterminate 0 sum 66… without units, tu "" count 1
/// indeterminate 0 sum 55… without units.
fn map_shard() -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 0,
        outcome: ok(),
        inputs: None,
        counters: vec![Counter { name: "digest.units".to_owned(), value: 3 }],
        digests: Some(digests_of(vec![
            listed(GroupClass::Declared, b"/a", vec![unit(0x41, 0x42, 0), unit(0x43, 0x44, 1)]),
            summed(GroupClass::Derived, b"", 1, d(0x66)),
            summed(GroupClass::Tu, b"", 1, d(0x55)),
        ])),
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: Some(slip_model::names::Names::default()),
    }
}

/// `group_digest` of the declared group of `map_shard`.
const MAP_GROUP: &str = "db3248c61386d5aa2e494aa55c9b780e301275eacc26c569cfd147b39300323d";
/// `tu_digest` of `map_shard`'s groups.
const MAP_TU: &str = "8ce2bbe9792bff3bfd8902d2ef50daaf4db1fbac564d05c70e26959a3cf88ec4";
const MAP_TRAILER: &str = "982cb92548bfb71f9a17c28c0b4af8106af98abb82a485c2a4231ecd1b1a12d7";

fn map_shard_bytes() -> Vec<u8> {
    hex(&[
        "53 4c 49 50 1a 0a 00 00  53 48 52 44  01 00 00 00", // magic, SHRD, version 1
        "01 22",
        &rep("33", 32),
        "01 00",              // header: observer, map, round 0
        "02 04  00 00 00 00", // outcome: ok, 0, 0, ""
        "04 0f  01  0c 64 69 67 65 73 74 2e 75 6e 69 74 73  03", // counters: digest.units = 3
        "13 96 02",           // digests, 278 bytes:
        "02",                 //   scheme 2      @78
        MAP_TU,               //   tu            @79
        "03",                 //   3 groups      @111
        "00  02 2f 61  02  01", //   declared "/a" count 2 (@116), indeterminate 1 (@117)   @112
        MAP_GROUP,            //     digest      @118
        "01 02",              //     units: 2    @150
        &rep("41", 32),
        &rep("42", 32),
        "00", //       (41…, 42…, 0)   @152
        &rep("43", 32),
        &rep("44", 32),
        "01",             //       (43…, 44…, 1)   @217
        "01  00  01  00", //   derived "" count 1 (@284), indeterminate 0 (@285)   @282
        &rep("66", 32),   //     digest
        "00",             //     no units      @318
        "02  00  01  00", //   tu "" count 1, indeterminate 0   @319
        &rep("55", 32),   //     digest
        "00",             //     no units
        "00",             // end                       @356
        MAP_TRAILER,
    ])
}

/// A verify shard: observer 33…, round 1, ok, no counters; digests with one
/// group, declared "/a" with units [(41…, 42…, 0)]; a `mismatch` verdict
/// against pre shard 77… under rules 88…, `only_post` 1, used [99…], total 3,
/// and the first two findings: `changed` 41… in "/a", 42… → 45…, "f", at
/// `/a:2:5` `call` "g" → `/a:2:5` `call` "h"; `removed` 46… in "/a", pre 47…,
/// "x", no location.
fn verify_shard() -> Shard {
    let site = |what: &str| Site {
        file: b"/a".to_vec(),
        line: 2,
        column: 5,
        node: "call".to_owned(),
        what: what.to_owned(),
    };
    Shard {
        observer: d(0x33),
        phase: Phase::Verify,
        round: 1,
        outcome: ok(),
        inputs: None,
        counters: Vec::new(),
        digests: Some(digests_of(vec![listed(
            GroupClass::Declared,
            b"/a",
            vec![unit(0x41, 0x42, 0)],
        )])),
        verdict: Some(Verdict {
            kind: VerdictKind::Mismatch,
            pre: d(0x77),
            rules: d(0x88),
            reason: String::new(),
            only_pre: 0,
            only_post: 1,
            used: vec![d(0x99)],
            total: 3,
            findings: vec![
                Finding {
                    kind: FindingKind::Changed,
                    key: Some(d(0x41)),
                    file: b"/a".to_vec(),
                    pre: Some(d(0x42)),
                    post: Some(d(0x45)),
                    name: "f".to_owned(),
                    location: Some(Where { pre: site("g"), post: site("h") }),
                },
                Finding {
                    kind: FindingKind::Removed,
                    key: Some(d(0x46)),
                    file: b"/a".to_vec(),
                    pre: Some(d(0x47)),
                    post: None,
                    name: "x".to_owned(),
                    location: None,
                },
            ],
        }),
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: Some(slip_model::names::Names::default()),
    }
}

const VERIFY_GROUP: &str = "16616ca8c602bc6c8ebe3c4e0dcd9c1659ee02809421d7d815b05b0176c69cd6";
const VERIFY_TU: &str = "7caedd7cdd53009ed0e15c4fac970a603d974936dd94351078141828b4ecaadc";
const VERIFY_TRAILER: &str = "987491e0b22bfbb556631932a8535aa537ae5b1e9238946b57a57c108bb23071";

fn verify_shard_bytes() -> Vec<u8> {
    hex(&[
        "53 4c 49 50 1a 0a 00 00  53 48 52 44  01 00 00 00", // magic, SHRD, version 1
        "01 22",
        &rep("33", 32),
        "02 01",                // header: observer, verify, round 1
        "02 04  00 00 00 00",   // outcome: ok, 0, 0, ""
        "04 01  00",            // counters: []
        "13 8b 01",             // digests, 139 bytes:
        "02",                   //   scheme 2                 @64
        VERIFY_TU,              //   tu                       @65
        "01",                   //   1 group
        "00  02 2f 61  01  00", //   declared "/a" count 1, indeterminate 0   @98
        VERIFY_GROUP,           //     digest                 @104
        "01 01",                //     units: 1
        &rep("41", 32),
        &rep("42", 32),
        "00",           //       (41…, 42…, 0)        @138
        "15 b3 02",     // verdict, 307 bytes:
        "01",           //   mismatch                 @206
        &rep("77", 32), //   pre
        &rep("88", 32), //   rules
        "00",           //   reason ""                @271
        "00 01",        //   only_pre 0, only_post 1
        "01",           //   used: 1                  @274
        &rep("99", 32), //     99…                    @275
        "03",           //   total 3                  @307
        "02",           //   findings: 2              @308
        "00  01",       //   changed, key             @309
        &rep("41", 32),
        "02 2f 61  01", //     file "/a", pre         @343
        &rep("42", 32),
        "01", //     post                   @379
        &rep("45", 32),
        "01 66  01",                              //     name "f", where        @412
        "02 2f 61  02 05  04 63 61 6c 6c  01 67", // pre: /a:2:5 call "g"   @415
        "02 2f 61  02 05  04 63 61 6c 6c  01 68", // post: /a:2:5 call "h"  @427
        "01  01",                                 //   removed, key             @439
        &rep("46", 32),
        "02 2f 61  01", //     file "/a", pre         @473
        &rep("47", 32),
        "00  01 78  00", //     no post, name "x", no where   @509
        "00",            // end                        @513
        VERIFY_TRAILER,
    ])
}

// spec/digest.md §11, spec/model.md §2, §5, §8: a map shard with digests,
// byte for byte.
#[test]
fn digests_bytes_exact() {
    let bytes = map_shard_bytes();
    assert_eq!(bytes.len(), 389);
    assert_eq!(map_shard().to_bytes(), bytes);
    assert_eq!(raw(&map_shard()), bytes);
    assert_eq!(Shard::from_bytes(&bytes).unwrap(), map_shard());
}

// spec/verify.md §6, spec/model.md §2, §5, §8: a verify shard with a
// mismatch verdict, byte for byte.
#[test]
fn verdict_bytes_exact() {
    let bytes = verify_shard_bytes();
    assert_eq!(bytes.len(), 546);
    assert_eq!(verify_shard().to_bytes(), bytes);
    assert_eq!(raw(&verify_shard()), bytes);
    assert_eq!(Shard::from_bytes(&bytes).unwrap(), verify_shard());
}

// spec/model.md §3, spec/digest.md §11: the group and tu digests of the
// hand-written shards are the tagged digests of their hand-written
// preimages.
#[test]
fn group_and_tu_digest_preimages() {
    let tagged = |tag: &str, body: Vec<u8>| {
        let mut pre = tag.as_bytes().to_vec();
        pre.push(0);
        pre.extend(body);
        content_digest(&pre)
    };
    let group = tagged(
        GROUP_TAG,
        hex(&["02", &rep("41", 32), &rep("42", 32), "00", &rep("43", 32), &rep("44", 32), "01"]),
    );
    assert_eq!(group.to_string(), MAP_GROUP);
    let tu = tagged(
        TU_TAG,
        hex(&[
            "03",
            "00 02 2f 61 02 01",
            MAP_GROUP,
            "01 00 01 00",
            &rep("66", 32),
            "02 00 01 00",
            &rep("55", 32),
        ]),
    );
    assert_eq!(tu.to_string(), MAP_TU);
    let group = tagged(GROUP_TAG, hex(&["01", &rep("41", 32), &rep("42", 32), "00"]));
    assert_eq!(group.to_string(), VERIFY_GROUP);
    let tu = tagged(TU_TAG, hex(&["01", "00 02 2f 61 01 00", VERIFY_GROUP]));
    assert_eq!(tu.to_string(), VERIFY_TU);
}

// ---------------------------------------------------------------------------
// Golden JSON (spec/model.md §10)

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

// spec/digest.md §11, spec/model.md §10: the rendering of a map shard with
// digests, pinned.
#[test]
fn golden_shard_with_digests() {
    let bytes = map_shard_bytes();
    let text = dump(&bytes).unwrap();
    golden("shard-digests.json", &text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(Shard::from_json(&parse(&text)).unwrap(), map_shard());
}

// spec/verify.md §6, spec/model.md §10: the rendering of a verify shard with
// a mismatch verdict, pinned.
#[test]
fn golden_shard_with_verdict() {
    let bytes = verify_shard_bytes();
    let text = dump(&bytes).unwrap();
    golden("shard-verdict.json", &text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(Shard::from_json(&parse(&text)).unwrap(), verify_shard());
}

// spec/model.md §8, §10: WP-006's golden shard, which has neither section,
// still reads and writes the same bytes.
#[test]
fn wp006_golden_shard_unchanged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/shard.json");
    let text = fs::read_to_string(path).unwrap();
    let bytes = undump(&parse(&text)).unwrap();
    assert_eq!(dump(&bytes).unwrap(), text);
    let s = Shard::from_bytes(&bytes).unwrap();
    assert_eq!((s.digests.as_ref(), s.verdict.as_ref()), (None, None));
    assert!(!text.contains("\"digests\"") && !text.contains("\"verdict\""));
    // Without its ignorable section, it is the typed value's bytes.
    assert_eq!(Shard::from_json(&parse(&text)).unwrap().to_bytes(), s.to_bytes());
}

// ---------------------------------------------------------------------------
// Round trips (spec/model.md §10: binary → JSON → binary reproduces the bytes)

fn round_trip(s: &Shard) {
    s.validate().unwrap();
    let bytes = s.to_bytes();
    assert_eq!(bytes, raw(s), "to_bytes agrees with the test's encoder");
    let back = Shard::from_bytes(&bytes).unwrap();
    assert_eq!(&back, s);
    assert_eq!(back.to_bytes(), bytes);
    let text = json::write_pretty(&s.to_json()).unwrap();
    let from_text = Shard::from_json(&parse(&text)).unwrap();
    assert_eq!(&from_text, s);
    assert_eq!(from_text.to_bytes(), bytes, "binary → JSON → binary");
    assert_eq!(dump(&bytes).unwrap(), text);
    assert_eq!(undump(&parse(&text)).unwrap(), bytes);
    assert_eq!(content_id(&bytes).unwrap().as_bytes()[..], bytes[bytes.len() - 32..]);
}

/// Groups of every class, with a non-UTF-8 file, a count and an
/// indeterminate at or above 2^53 (unlisted), and the unit lists of the
/// groups `listed_at` names.
fn sample_digests(listed_at: &[usize]) -> Digests {
    let groups: Vec<(GroupClass, &[u8], Vec<Unit>)> = vec![
        (GroupClass::Declared, b"/s/a.h", vec![unit(1, 2, 0), unit(1, 3, 0), unit(5, 6, 1)]),
        (GroupClass::Declared, b"/s/\xffb.h", vec![unit(7, 8, 0)]),
        (GroupClass::Derived, b"", vec![unit(9, 10, 0), unit(11, 12, 0)]),
        (GroupClass::Derived, b"/s/a.h", vec![unit(13, 14, 1)]),
        (GroupClass::Tu, b"", vec![unit(15, 16, 0), unit(17, 18, 0)]),
    ];
    let mut groups: Vec<Group> = groups
        .into_iter()
        .enumerate()
        .map(|(i, (class, file, units))| {
            let mut g = listed(class, file, units);
            if !listed_at.contains(&i) {
                g.units = None;
            }
            g
        })
        .collect();
    if listed_at.len() < 5 {
        let big = Group {
            indeterminate: u64::MAX,
            ..summed(GroupClass::Derived, b"/s/z.h", u64::MAX, d(0xee))
        };
        groups.insert(4, big);
    }
    digests_of(groups)
}

fn sample_map_shard(digests: Digests) -> Shard {
    Shard {
        observer: d(0x30),
        phase: Phase::Map,
        round: 2,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 4, message: String::new() },
        inputs: Some(vec![FileRef { path: b"/s/a.cc".to_vec(), size: 9, content: d(1) }]),
        counters: vec![Counter { name: "digest.groups".to_owned(), value: 5 }],
        digests: Some(digests),
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: Some(slip_model::names::Names::default()),
    }
}

fn site(file: &[u8], line: u64, column: u64, node: &str, what: &str) -> Site {
    Site { file: file.to_vec(), line, column, node: node.to_owned(), what: what.to_owned() }
}

fn finding(kind: FindingKind, key: Option<u8>, file: &[u8]) -> Finding {
    Finding {
        kind,
        key: key.map(d),
        file: file.to_vec(),
        pre: None,
        post: None,
        name: String::new(),
        location: None,
    }
}

/// A mismatch with a finding of every kind, with and without `where`,
/// non-UTF-8 files, an unnamed indeterminate finding, values above 2^53;
/// in the order of findings, by rank (`derived_changed` before
/// `indeterminate`, spec/verify.md §6).
fn sample_mismatch() -> Verdict {
    let both = |pre: Site, post: Site| Some(Where { pre, post });
    let findings = vec![
        Finding {
            pre: Some(d(0xa1)),
            post: Some(d(0xa2)),
            name: "ns::f(int)".to_owned(),
            location: both(
                site(b"/s/a.h", 10, 3, "call", "ns::g"),
                site(b"/s/a.h", 10, 3, "call", "ns::h"),
            ),
            ..finding(FindingKind::Changed, Some(0x10), b"/s/a.h")
        },
        Finding {
            pre: Some(d(0xa3)),
            post: Some(d(0xa4)),
            location: both(site(b"/s/\xffb.h", 1, 1, "type", ""), site(b"", 0, 0, "", "")),
            ..finding(FindingKind::Changed, Some(0x11), b"/s/\xffb.h")
        },
        Finding {
            pre: Some(d(0xa5)),
            name: "ns::old".to_owned(),
            location: both(site(b"/s/a.h", 4, 1, "var", ""), Site::default()),
            ..finding(FindingKind::Removed, Some(0x12), b"/s/a.h")
        },
        Finding { post: Some(d(0xa6)), ..finding(FindingKind::Added, Some(0x13), b"/s/a.h") },
        Finding {
            pre: Some(d(0xa8)),
            post: Some(d(0xa9)),
            ..finding(FindingKind::DerivedChanged, Some(0x15), b"")
        },
        finding(FindingKind::Indeterminate, None, b""),
        finding(FindingKind::Indeterminate, None, b""),
        finding(FindingKind::Indeterminate, Some(0x14), b""),
        Finding {
            pre: Some(d(0xa7)),
            ..finding(FindingKind::Indeterminate, Some(0x17), b"/s/a.h")
        },
        finding(FindingKind::DerivedIndeterminate, Some(0x16), b"/s/a.h"),
    ];
    Verdict {
        kind: VerdictKind::Mismatch,
        pre: d(0x70),
        rules: d(0x71),
        reason: String::new(),
        only_pre: 3,
        only_post: 1 << 53,
        used: vec![d(0x01), d(0x02)],
        total: 1 << 60,
        findings,
    }
}

fn sample_verify_shard(verdict: Verdict) -> Shard {
    Shard {
        phase: Phase::Verify,
        verdict: Some(verdict),
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        ..sample_map_shard(sample_digests(&[0, 3]))
    }
}

fn verified() -> Verdict {
    Verdict {
        kind: VerdictKind::Verified,
        pre: d(0x70),
        rules: d(0x71),
        reason: String::new(),
        only_pre: 0,
        only_post: 0,
        used: vec![d(0x05)],
        total: 0,
        findings: Vec::new(),
    }
}

fn unverifiable() -> Verdict {
    Verdict {
        kind: VerdictKind::Unverifiable,
        pre: Digest([0; 32]),
        reason: "the pre shard has no digests section".to_owned(),
        used: Vec::new(),
        ..verified()
    }
}

// spec/digest.md §11, spec/model.md §10: digests without unit lists, with
// some, with all; no groups; a non-UTF-8 file.
#[test]
fn digests_round_trip() {
    for listed_at in [&[][..], &[0, 3], &[0, 1, 2, 3, 4]] {
        round_trip(&sample_map_shard(sample_digests(listed_at)));
    }
    round_trip(&sample_map_shard(digests_of(Vec::new())));
    round_trip(&map_shard());
    // A verify shard may carry digests without a verdict.
    round_trip(&Shard { phase: Phase::Verify, ..map_shard() });
    let v = sample_map_shard(sample_digests(&[])).to_json();
    let groups = v.get("sections").unwrap().get("digests").unwrap().get("groups").unwrap();
    let groups = groups.as_array().unwrap();
    assert_eq!(groups[1].get("file"), Some(&parse(r#"{"hex": "2f732fff622e68"}"#)));
    assert_eq!(groups[4].get("count"), Some(&Value::Str("18446744073709551615".into())));
    assert_eq!(groups[4].get("indeterminate"), Some(&Value::Str("18446744073709551615".into())));
    assert_eq!(groups[0].get("indeterminate"), Some(&Value::Int(1)));
    assert_eq!(groups[4].get("units"), Some(&Value::Null));
    assert_eq!(groups[5].get("class"), Some(&Value::Str("tu".into())));
}

// spec/verify.md §6, spec/model.md §10: each verdict kind; findings of every
// kind, with and without `where`; non-UTF-8 files; a reason and names at
// the 4096-byte limit.
#[test]
fn verdict_round_trip() {
    round_trip(&verify_shard());
    for v in [verified(), unverifiable(), sample_mismatch()] {
        round_trip(&sample_verify_shard(v));
    }
    let mut long = unverifiable();
    long.reason = "é".repeat(2048);
    round_trip(&sample_verify_shard(long));
    let mut long = sample_mismatch();
    long.findings[0].name = "n".repeat(4096);
    let w = long.findings[0].location.as_mut().unwrap();
    w.pre.node = "o".repeat(4096);
    w.post.what = "w".repeat(4096);
    round_trip(&sample_verify_shard(long));
    // A mismatch may hold fewer findings than its total, even none.
    let mut none = sample_mismatch();
    none.findings.clear();
    round_trip(&sample_verify_shard(none));

    let v = sample_verify_shard(sample_mismatch()).to_json();
    let verdict = v.get("sections").unwrap().get("verdict").unwrap();
    assert_eq!(verdict.get("only_post"), Some(&Value::Str("9007199254740992".into())));
    let f = verdict.get("findings").unwrap().as_array().unwrap();
    assert_eq!(f[1].get("file"), Some(&parse(r#"{"hex": "2f732fff622e68"}"#)));
    let w = f[1].get("where").unwrap();
    assert_eq!(w.get("pre").unwrap().get("file"), Some(&parse(r#"{"hex": "2f732fff622e68"}"#)));
    assert_eq!(f[3].get("where"), Some(&Value::Null));
    assert_eq!(f[3].get("pre"), Some(&Value::Null));
    assert_eq!(f[4].get("kind"), Some(&Value::Str("derived_changed".into())));
    assert_eq!(f[5].get("key"), Some(&Value::Null));
    assert_eq!(f[5].get("kind"), Some(&Value::Str("indeterminate".into())));
    assert_eq!(f[9].get("kind"), Some(&Value::Str("derived_indeterminate".into())));
}

// ---------------------------------------------------------------------------
// Invariants (docs/wp/WP-015.md; offsets and paths:
// spec/model.md §8.2, §8.3): each rejected by from_bytes (Malformed at
// the offset), by validate and by from_json (at the path).

/// The offset at which `from_bytes` rejects `s`'s file, after checking
/// that `validate`, `from_json` and `undump` reject it at `path` with the
/// same message.
fn rejected(s: &Shard, path: &str) -> usize {
    let e = Shard::from_bytes(&raw(s)).expect_err("from_bytes accepted an invalid shard");
    assert_eq!(e.kind, ErrorKind::Malformed, "{e}");
    let v = s.validate().expect_err("validate accepted an invalid shard");
    assert_eq!(v, format!("{path}: {}", e.what), "validate");
    let j = Shard::from_json(&s.to_json()).expect_err("from_json accepted an invalid shard");
    assert_eq!(j, JsonFormError { path: path.to_owned(), message: e.what.to_owned() });
    assert_eq!(undump(&s.to_json()).unwrap_err(), j);
    e.offset
}

fn groups(s: &mut Shard) -> &mut Vec<Group> {
    &mut s.digests.as_mut().unwrap().groups
}

fn units(s: &mut Shard, i: usize) -> &mut Vec<Unit> {
    groups(s)[i].units.as_mut().unwrap()
}

fn verdict(s: &mut Shard) -> &mut Verdict {
    s.verdict.as_mut().unwrap()
}

// digests rule 1 (spec/digest.md §10): only in a shard whose status is ok.
#[test]
fn digests_1_only_for_status_ok() {
    for status in [Status::Diagnosed, Status::Rejected] {
        let mut s = map_shard();
        s.outcome.status = status;
        assert_eq!(rejected(&s, "sections.digests"), 78);
    }
    // A failed shard has no counters; the section moves up by 14 bytes.
    let mut s = map_shard();
    s.outcome.status = Status::Failed;
    s.counters.clear();
    assert_eq!(rejected(&s, "sections.digests"), 78 - 14);
}

// digests rule 2 (spec/digest.md §11): the scheme is SCHEME.
#[test]
fn digests_2_scheme() {
    for scheme in [0, 1, 3] {
        let mut s = map_shard();
        s.digests.as_mut().unwrap().scheme = scheme;
        assert_eq!(rejected(&s, "sections.digests.scheme"), 78);
    }
}

// digests rule 3 (spec/digest.md §11): groups strictly ascending by
// (class, file).
#[test]
fn digests_3_groups_ascending() {
    let mut s = map_shard();
    groups(&mut s).swap(1, 2);
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[2]"), 319);
    let mut s = map_shard();
    groups(&mut s).swap(0, 1);
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[1]"), 149);
    // Equal (class, file) twice.
    let mut s = map_shard();
    groups(&mut s)[2].class = GroupClass::Derived;
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[2]"), 319);
    // By file within a class, a proper prefix first.
    let mut s = sample_map_shard(sample_digests(&[]));
    groups(&mut s)[0].file = b"/s/a.hh".to_vec();
    groups(&mut s)[1].file = b"/s/a.h".to_vec();
    resum(&mut s);
    rejected(&s, "sections.digests.groups[1]");
}

// digests rule 4 (spec/digest.md §11): a tu group has an empty file, a
// declared group a non-empty one.
#[test]
fn digests_4_group_files() {
    let mut s = map_shard();
    groups(&mut s)[2].file = b"/t".to_vec();
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[2].file"), 320);
    let mut s = map_shard();
    groups(&mut s)[0].file.clear();
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[0].file"), 113);
    // A derived group may have either (builtins have the empty path).
    let mut s = map_shard();
    groups(&mut s)[1].file = b"/b".to_vec();
    resum(&mut s);
    round_trip(&s);
}

// digests rule 5a (spec/digest.md §11): count at least 1.
#[test]
fn digests_5a_count_at_least_1() {
    let mut s = map_shard();
    groups(&mut s)[1].count = 0;
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[1].count"), 284);
}

// digests rule 5b (spec/digest.md §11, WP-017): `indeterminate` is at most
// `count`, also for a group without a unit list.
#[test]
fn digests_5b_indeterminate_at_most_count() {
    let mut s = map_shard();
    groups(&mut s)[1].indeterminate = 2;
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[1].indeterminate"), 285);
    let mut s = map_shard();
    groups(&mut s)[2].indeterminate = u64::MAX;
    resum(&mut s);
    // count 1, indeterminate u64::MAX (10 bytes) at 322.
    assert_eq!(rejected(&s, "sections.digests.groups[2].indeterminate"), 322);
    // Equal to count is allowed.
    let mut s = map_shard();
    groups(&mut s)[1].indeterminate = 1;
    resum(&mut s);
    round_trip(&s);
    // Checked over every group before rule 6: group 1 breaks 5b, group 0
    // (earlier) breaks 6b.
    let mut s = map_shard();
    groups(&mut s)[0].count = 3;
    groups(&mut s)[1].indeterminate = 2;
    resum(&mut s);
    rejected(&s, "sections.digests.groups[1].indeterminate");
}

// digests rule 6 (spec/digest.md §11): a unit list is strictly ascending by
// (key, digest).
#[test]
fn digests_6_units_ascending() {
    let mut s = map_shard();
    units(&mut s, 0).swap(0, 1);
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[0].units[1]"), 217);
    // The same key and digest twice is one unit (§11), whatever the flags.
    let mut s = map_shard();
    units(&mut s, 0)[1] = unit(0x41, 0x42, 1);
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[0].units[1]"), 217);
    // Same key, different digests: ascending by digest.
    let mut s = map_shard();
    units(&mut s, 0)[1] = unit(0x41, 0x40, 0);
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[0].units[1]"), 217);
}

// digests rule 6 (spec/digest.md §11): the list's length is the count.
#[test]
fn digests_6_units_length_is_count() {
    for count in [1, 3] {
        let mut s = map_shard();
        groups(&mut s)[0].count = count;
        resum(&mut s);
        assert_eq!(rejected(&s, "sections.digests.groups[0].units"), 151);
    }
    // An empty list against a count of 1 (rule 5 holds).
    let mut s = map_shard();
    groups(&mut s)[1].units = Some(Vec::new());
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[1].units"), 319);
}

// digests rule 6 (spec/digest.md §11): flags are 0 or 1.
#[test]
fn digests_6_unit_flags() {
    for flags in [2, 3, u64::MAX] {
        let mut s = map_shard();
        units(&mut s, 0)[1].flags = flags;
        resum(&mut s);
        assert_eq!(rejected(&s, "sections.digests.groups[0].units[1].flags"), 281);
    }
}

// digests rule 6d (spec/digest.md §11, WP-017): a listed group's
// `indeterminate` is the number of its units with the flag.
#[test]
fn digests_6d_indeterminate_is_the_number_flagged() {
    for n in [0, 2] {
        let mut s = map_shard();
        groups(&mut s)[0].indeterminate = n;
        resum(&mut s);
        assert_eq!(rejected(&s, "sections.digests.groups[0].indeterminate"), 117, "{n}");
    }
    // The flag cleared, the count kept.
    let mut s = map_shard();
    units(&mut s, 0)[1].flags = 0;
    resum(&mut s);
    assert_eq!(rejected(&s, "sections.digests.groups[0].indeterminate"), 117);
    // And both changed together is consistent.
    groups(&mut s)[0].indeterminate = 0;
    resum(&mut s);
    round_trip(&s);
    // Checked after 6c (flags) and before 6e (digest).
    let mut s = map_shard();
    units(&mut s, 0)[1].flags = 2;
    resum(&mut s);
    rejected(&s, "sections.digests.groups[0].units[1].flags");
    let mut s = map_shard();
    groups(&mut s)[0].indeterminate = 0;
    resum(&mut s);
    groups(&mut s)[0].digest = d(0x01);
    s.digests.as_mut().unwrap().tu = tu_digest(&s.digests.as_ref().unwrap().groups);
    rejected(&s, "sections.digests.groups[0].indeterminate");
    // A group without a unit list is not checked against its units.
    let mut s = map_shard();
    groups(&mut s)[1].indeterminate = 1;
    resum(&mut s);
    round_trip(&s);
}

// digests rule 6e (spec/digest.md §11): a listed group's digest is
// group_digest of its units, recomputed by the reader.
#[test]
fn digests_6e_group_digest_recomputed() {
    let mut s = map_shard();
    groups(&mut s)[0].digest = d(0x01);
    s.digests.as_mut().unwrap().tu = tu_digest(&s.digests.as_ref().unwrap().groups);
    assert_eq!(rejected(&s, "sections.digests.groups[0].digest"), 118);
    // The reader computes it: flipping a unit's digest byte in the file,
    // with the trailer resealed, is detected.
    let bytes = map_shard_bytes();
    let bad = patch(&bytes, 184, 0x42, 0x43);
    assert_eq!(error(Shard::from_bytes(&bad)), (ErrorKind::Malformed, 118));
}

// digests rule 7 (spec/digest.md §11): tu is tu_digest of the groups,
// recomputed by the reader.
#[test]
fn digests_7_tu_recomputed() {
    let mut s = map_shard();
    s.digests.as_mut().unwrap().tu = d(0x01);
    assert_eq!(rejected(&s, "sections.digests.tu"), 79);
    // A group's sum changed in the file is detected through tu.
    let bad = patch(&map_shard_bytes(), 284, 0x01, 0x02);
    assert_eq!(error(Shard::from_bytes(&bad)), (ErrorKind::Malformed, 79));
    // `indeterminate` is part of the sum (WP-017): changed in a group
    // without a unit list, within its count, it is detected through tu.
    let bad = patch(&map_shard_bytes(), 285, 0x00, 0x01);
    assert_eq!(error(Shard::from_bytes(&bad)), (ErrorKind::Malformed, 79));
}

// verdict rule 1 (spec/verify.md §6): only in a verify shard whose status is
// ok and that has digests.
#[test]
fn verdict_1_where_it_may_be() {
    let mut s = verify_shard();
    s.phase = Phase::Map;
    assert_eq!(rejected(&s, "sections.verdict"), 206);
    let mut s = verify_shard();
    s.digests = None;
    s.names = None;
    // Without the 142 bytes of the digests section.
    assert_eq!(rejected(&s, "sections.verdict"), 206 - 142);
    let mut s = verify_shard();
    s.digests = None;
    s.names = None;
    s.outcome.status = Status::Rejected;
    let offset = rejected(&s, "sections.verdict");
    assert_eq!(
        s.validate().unwrap_err(),
        "sections.verdict: a verdict section in a shard whose status is not ok"
    );
    assert_eq!(offset, 206 - 142);
}

// verdict rule 2 (spec/verify.md §6, spec/model.md §8): reason, name, node
// and what are at most 4096 bytes.
#[test]
fn verdict_2_strings_at_most_4096_bytes() {
    let mut s = verify_shard();
    *verdict(&mut s) = Verdict { reason: "r".repeat(4097), ..unverifiable() };
    verdict(&mut s).pre = d(0x77);
    assert_eq!(rejected(&s, "sections.verdict.reason"), 271);
    let mut s = verify_shard();
    verdict(&mut s).findings[1].name = "é".repeat(2049);
    assert_eq!(rejected(&s, "sections.verdict.findings[1].name"), 510);
    for (side, field, offset) in
        [("pre", "node", 420), ("pre", "what", 425), ("post", "node", 432), ("post", "what", 437)]
    {
        let mut s = verify_shard();
        let w = verdict(&mut s).findings[0].location.as_mut().unwrap();
        let site = if side == "pre" { &mut w.pre } else { &mut w.post };
        let long = "x".repeat(4097);
        if field == "node" {
            site.node = long;
        } else {
            site.what = long;
        }
        let path = format!("sections.verdict.findings[0].where.{side}.{field}");
        assert_eq!(rejected(&s, &path), offset, "{path}");
    }
}

// verdict rule 3 (spec/verify.md §6): what each kind of verdict holds.
#[test]
fn verdict_3_kinds() {
    let with = |v: Verdict| Shard { verdict: Some(v), ..verify_shard() };
    // verified: empty reason, total 0, no finding.
    let s = with(Verdict { reason: "x".into(), ..verified() });
    rejected(&s, "sections.verdict.reason");
    let s = with(Verdict { total: 1, ..verified() });
    rejected(&s, "sections.verdict.total");
    let s = with(Verdict { findings: verify_shard().verdict.unwrap().findings, ..verified() });
    rejected(&s, "sections.verdict.findings");
    // mismatch: empty reason, total at least 1.
    let mut s = verify_shard();
    verdict(&mut s).reason = "x".into();
    assert_eq!(rejected(&s, "sections.verdict.reason"), 271);
    let mut s = verify_shard();
    verdict(&mut s).total = 0;
    verdict(&mut s).findings.clear();
    rejected(&s, "sections.verdict.total");
    // unverifiable: a reason, total 0, no finding.
    let s = with(Verdict { reason: String::new(), ..unverifiable() });
    rejected(&s, "sections.verdict.reason");
    let s = with(Verdict { total: 2, ..unverifiable() });
    rejected(&s, "sections.verdict.total");
    let s = with(Verdict { findings: verify_shard().verdict.unwrap().findings, ..unverifiable() });
    rejected(&s, "sections.verdict.findings");
    // Offsets in the hand-written file.
    let mut s = verify_shard();
    verdict(&mut s).kind = VerdictKind::Verified;
    assert_eq!(rejected(&s, "sections.verdict.total"), 307);
    let mut s = verify_shard();
    verdict(&mut s).kind = VerdictKind::Verified;
    verdict(&mut s).total = 0;
    assert_eq!(rejected(&s, "sections.verdict.findings"), 308);
    let mut s = verify_shard();
    verdict(&mut s).kind = VerdictKind::Unverifiable;
    verdict(&mut s).reason = "r".into();
    verdict(&mut s).total = 0;
    // The reason moves `total` and `findings` one byte up.
    assert_eq!(rejected(&s, "sections.verdict.findings"), 309);
}

// verdict rule 3, `pre` (spec/model.md §8.3): a verified or mismatch
// verdict compared against a pre shard, so its `pre` is not all zero; only
// an unverifiable one may have none. Checked first, `pre` being the first
// field after `kind`; binary (offset 207, the `pre` field) and JSON.
#[test]
fn verdict_3_pre_shard() {
    let zero = Digest([0; 32]);
    let mut s = verify_shard();
    verdict(&mut s).pre = zero;
    assert_eq!(rejected(&s, "sections.verdict.pre"), 207);
    let e = Shard::from_bytes(&raw(&s)).unwrap_err();
    assert_eq!(e.what, "a mismatch verdict without a pre shard");
    // Before the reason of rule 3.
    verdict(&mut s).reason = "x".into();
    assert_eq!(rejected(&s, "sections.verdict.pre"), 207);
    let mut s = verify_shard();
    verdict(&mut s).kind = VerdictKind::Verified;
    verdict(&mut s).total = 0;
    verdict(&mut s).findings.clear();
    verdict(&mut s).pre = zero;
    // Without findings the section's length takes one byte less: `pre` is
    // at 206.
    assert_eq!(rejected(&s, "sections.verdict.pre"), 206);
    let e = Shard::from_bytes(&raw(&s)).unwrap_err();
    assert_eq!(e.what, "a verified verdict without a pre shard");
    // Unverifiable: with or without a pre shard.
    let with = |v: Verdict| Shard { verdict: Some(v), ..verify_shard() };
    round_trip(&with(Verdict { pre: zero, ..unverifiable() }));
    round_trip(&with(Verdict { pre: d(0x70), ..unverifiable() }));
}

// verdict rule 4 (spec/verify.md §6): no more findings than total.
#[test]
fn verdict_4_findings_within_total() {
    let mut s = verify_shard();
    verdict(&mut s).total = 1;
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 439);
    let mut s = verify_shard();
    verdict(&mut s).total = 2;
    round_trip(&s);
}

// verdict rule 4b (spec/verify.md §6, spec/model.md §8.3): ascending by
// (rank, file, key), an absent key first; at most one finding per (kind, key).
#[test]
fn verdict_4_findings_order_and_uniqueness() {
    // By kind.
    let mut s = verify_shard();
    verdict(&mut s).findings.swap(0, 1);
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 309 + 74);
    // By file within a kind.
    let mut s = verify_shard();
    verdict(&mut s).findings[1].kind = FindingKind::Changed;
    verdict(&mut s).findings[0].file = b"/b".to_vec();
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 439);
    // By key within a kind and file, an absent key first.
    let mut v = sample_mismatch();
    v.findings.swap(6, 7);
    rejected(&sample_verify_shard(v), "sections.verdict.findings[7]");
    let mut v = sample_mismatch();
    v.findings.swap(0, 1);
    v.findings[0].file = b"/s/a.h".to_vec();
    rejected(&sample_verify_shard(v), "sections.verdict.findings[1]");
    // The same kind and key twice, in different files.
    let mut s = verify_shard();
    verdict(&mut s).findings[1].kind = FindingKind::Changed;
    verdict(&mut s).findings[1].key = Some(d(0x41));
    verdict(&mut s).findings[1].file = b"/b".to_vec();
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 439);
    // The same kind and key in the same file, and in another file.
    let mut v = sample_mismatch();
    v.findings[8].key = Some(d(0x14));
    rejected(&sample_verify_shard(v.clone()), "sections.verdict.findings[8]");
    v.findings[8].file.clear();
    rejected(&sample_verify_shard(v), "sections.verdict.findings[8]");
    // Unnamed indeterminate findings may repeat; a key under two kinds is
    // two findings.
    let mut v = sample_mismatch();
    v.findings[9].key = Some(d(0x15));
    round_trip(&sample_verify_shard(v));
}

// verdict rule 4b (spec/model.md §8.3, spec/verify.md §6): the order is by
// rank, not by the kind's number. A `derived_changed` finding (number 4,
// rank 3) before an `indeterminate` one (number 3, rank 4) is accepted in
// binary and JSON; the reverse, the former order by number, is rejected at
// the later finding's first byte and path, by every reader.
#[test]
fn verdict_4b_order_by_rank() {
    let kinds = |a: FindingKind, b: FindingKind| {
        let mut s = verify_shard();
        let v = verdict(&mut s);
        v.findings[0].kind = a;
        v.findings[1].kind = b;
        v.findings[1].post = Some(d(0x48));
        s
    };
    round_trip(&kinds(FindingKind::DerivedChanged, FindingKind::Indeterminate));
    round_trip(&kinds(FindingKind::DerivedChanged, FindingKind::DerivedIndeterminate));
    let mut s = kinds(FindingKind::Added, FindingKind::DerivedChanged);
    verdict(&mut s).findings[0].pre = None;
    round_trip(&s);
    let s = kinds(FindingKind::Indeterminate, FindingKind::DerivedChanged);
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 439);
    let e = Shard::from_bytes(&raw(&s)).unwrap_err();
    assert_eq!(e.what, "findings are not ascending by (rank, file, key)");
    // Across files, the rank decides first: an indeterminate finding of
    // file "/a" is after a derived_changed one of file "/b".
    let mut s = kinds(FindingKind::DerivedChanged, FindingKind::Indeterminate);
    verdict(&mut s).findings[0].file = b"/b".to_vec();
    round_trip(&s);
    let mut s = kinds(FindingKind::Indeterminate, FindingKind::DerivedChanged);
    verdict(&mut s).findings[1].file = b"/b".to_vec();
    assert_eq!(rejected(&s, "sections.verdict.findings[1]"), 439);
}

// spec/verify.md §6: `FindingKind::rank` is the table of §6, a bijection
// onto 0..=5, and every kind's number in the format is what it was (the
// kind byte of a finding in binary, its name in JSON).
#[test]
fn finding_kind_rank_and_number() {
    use FindingKind::*;
    let table = [
        (Changed, 0u8, 0u8, "changed"),
        (Removed, 1, 1, "removed"),
        (Added, 2, 2, "added"),
        (Indeterminate, 3, 4, "indeterminate"),
        (DerivedChanged, 4, 3, "derived_changed"),
        (DerivedIndeterminate, 5, 5, "derived_indeterminate"),
    ];
    let mut ranks: Vec<u8> = table.iter().map(|&(k, _, _, _)| k.rank()).collect();
    ranks.sort_unstable();
    assert_eq!(ranks, [0, 1, 2, 3, 4, 5]);
    for (kind, number, rank, name) in table {
        assert_eq!(kind.rank(), rank, "{kind:?}");
        let mut s = verify_shard();
        let v = verdict(&mut s);
        v.findings.truncate(1);
        v.findings[0].kind = kind;
        // The first finding starts at byte 309 (verdict_4_findings_order_and_uniqueness).
        assert_eq!(raw(&s)[309], number, "{kind:?}");
        let json = s.to_json();
        let f = &json.get("sections").unwrap().get("verdict").unwrap().get("findings").unwrap();
        assert_eq!(f.as_array().unwrap()[0].get("kind"), Some(&Value::Str(name.into())));
    }
}

// verdict rule 5 (spec/verify.md §6): a key is absent only for
// indeterminate; removed has no post, added no pre.
#[test]
fn verdict_5_finding_fields() {
    for kind in [
        FindingKind::Changed,
        FindingKind::Removed,
        FindingKind::Added,
        FindingKind::DerivedChanged,
        FindingKind::DerivedIndeterminate,
    ] {
        let mut s = verify_shard();
        let f = &mut verdict(&mut s).findings[1];
        f.kind = kind;
        f.key = None;
        f.pre = None;
        // After finding 0 in every order.
        f.file = b"/b".to_vec();
        assert_eq!(rejected(&s, "sections.verdict.findings[1].key"), 440, "{kind:?}");
    }
    let mut s = verify_shard();
    verdict(&mut s).findings[1].kind = FindingKind::Added;
    assert_eq!(rejected(&s, "sections.verdict.findings[1].pre"), 476);
    let mut s = verify_shard();
    verdict(&mut s).findings[1].post = Some(d(0x48));
    assert_eq!(rejected(&s, "sections.verdict.findings[1].post"), 509);
}

// verdict rule 6 (spec/verify.md §6): used strictly ascending.
#[test]
fn verdict_6_used_ascending() {
    let mut s = verify_shard();
    verdict(&mut s).used.push(d(0x99));
    assert_eq!(rejected(&s, "sections.verdict.used[1]"), 307);
    let mut s = verify_shard();
    verdict(&mut s).used.push(d(0x98));
    assert_eq!(rejected(&s, "sections.verdict.used[1]"), 307);
}

// verdict rule 7 (spec/verify.md §6.1): line is 0 exactly when column is 0.
#[test]
fn verdict_7_site_line_and_column() {
    let mut s = verify_shard();
    verdict(&mut s).findings[0].location.as_mut().unwrap().pre.line = 0;
    assert_eq!(rejected(&s, "sections.verdict.findings[0].where.pre.line"), 418);
    let mut s = verify_shard();
    verdict(&mut s).findings[0].location.as_mut().unwrap().post.column = 0;
    assert_eq!(rejected(&s, "sections.verdict.findings[0].where.post.line"), 430);
}

// ---------------------------------------------------------------------------
// Structural rejections (spec/model.md §1 rule 6, §2, §5)

// spec/model.md §5 rule 3, §2.1: a truncated section is Truncated at its
// end; leftover bytes are Malformed at the first of them.
#[test]
fn truncated_sections_and_leftover_bytes() {
    for (shard, id, leftover) in
        [(map_shard(), 19, 356), (verify_shard(), 19, 203), (verify_shard(), 21, 513)]
    {
        let mut sections = raw_sections(&shard);
        let at = sections.iter().position(|s| s.0 == id).unwrap();
        for cut in [1, 33] {
            let mut short = sections.clone();
            let len = short[at].1.len();
            short[at].1.truncate(len - cut);
            let bytes = sealed(&short);
            assert_eq!(error(Shard::from_bytes(&bytes)).0, ErrorKind::Truncated, "{id} -{cut}");
            assert_eq!(error(dump(&bytes)).0, ErrorKind::Truncated);
        }
        sections[at].1.push(0);
        let bytes = sealed(&sections);
        assert_eq!(error(Shard::from_bytes(&bytes)), (ErrorKind::Malformed, leftover), "{id}");
    }
}

// spec/model.md §1 rule 6, §2: unlisted GroupClass, VerdictKind and
// FindingKind values, and opt tags other than 0 or 1, are Malformed at
// their first byte.
#[test]
fn unlisted_values_and_opt_tags() {
    let map = map_shard_bytes();
    for (at, from, to) in [(112, 0, 3), (282, 1, 0x7f), (150, 1, 2), (318, 0, 0xff)] {
        assert_eq!(
            error(Shard::from_bytes(&patch(&map, at, from, to))),
            (ErrorKind::Malformed, at)
        );
    }
    let verify = verify_shard_bytes();
    for (at, from, to) in [
        (206, 1, 3),
        (309, 0, 6),
        (439, 1, 0x40),
        (310, 1, 2),
        (379, 1, 2),
        (414, 1, 2),
        (512, 0, 2),
    ] {
        assert_eq!(
            error(Shard::from_bytes(&patch(&verify, at, from, to))),
            (ErrorKind::Malformed, at),
            "{at}"
        );
    }
    // A count larger than the bytes remaining is TooLarge, never allocated:
    // the groups count and the findings count replaced by 65535 with
    // nothing after it (the section's length now takes one byte, not two).
    for (shard, id, keep, at) in [(map_shard(), 19, 33, 110), (verify_shard(), 21, 102, 307)] {
        let mut sections = raw_sections(&shard);
        let s = sections.iter_mut().find(|s| s.0 == id).unwrap();
        s.1.truncate(keep);
        s.1.extend([0xff, 0xff, 0x03]);
        assert_eq!(error(Shard::from_bytes(&sealed(&sections))), (ErrorKind::TooLarge, at));
    }
    // JSON: names other than the spec's.
    for (path, at) in [
        (&["sections", "digests", "groups", "0", "class"][..], "sections.digests.groups[0].class"),
        (&["sections", "verdict", "kind"][..], "sections.verdict.kind"),
        (
            &["sections", "verdict", "findings", "1", "kind"][..],
            "sections.verdict.findings[1].kind",
        ),
    ] {
        let mut v = verify_shard().to_json();
        set(&mut v, path, Value::Str("Changed".into()));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, at);
    }
}

// spec/model.md §10: the JSON members are named and ordered as the spec's
// fields (`Finding.location` is `where`); a missing, unknown or misplaced
// member is an error at its path.
#[test]
fn json_members() {
    let v = verify_shard().to_json();
    let sections = v.get("sections").unwrap().as_object().unwrap();
    let names: Vec<&str> = sections.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["header", "outcome", "counters", "digests", "verdict"]);
    let f = v.get("sections").unwrap().get("verdict").unwrap().get("findings").unwrap();
    let f0: Vec<&str> =
        f.as_array().unwrap()[0].as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(f0, ["kind", "key", "file", "pre", "post", "name", "where"]);
    let g = v.get("sections").unwrap().get("digests").unwrap().get("groups").unwrap();
    let g0: Vec<&str> =
        g.as_array().unwrap()[0].as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(g0, ["class", "file", "count", "indeterminate", "digest", "units"]);
    // WP-017: `indeterminate` is required, after `count`.
    let text = json::write_pretty(&verify_shard().to_json()).unwrap();
    assert_eq!(text.matches("\"indeterminate\": 0,").count(), 1);
    for (edit, at) in [
        (text.replace("\"indeterminate\": 0,", ""), "sections.digests.groups[0].indeterminate"),
        (
            text.replace("\"indeterminate\": 0,", "\"indeterminate\": -1,"),
            "sections.digests.groups[0].indeterminate",
        ),
    ] {
        assert_eq!(Shard::from_json(&parse(&edit)).unwrap_err().path, at);
    }

    let mut v = verify_shard().to_json();
    set(&mut v, &["sections", "verdict", "findings", "0", "where"], parse(r#"{"pre": null}"#));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.verdict.findings[0].where.post");
    let mut v = verify_shard().to_json();
    set(&mut v, &["sections", "digests", "groups", "0", "units"], parse("{}"));
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.digests.groups[0].units");
    let mut v = verify_shard().to_json();
    set(&mut v, &["sections", "verdict", "used", "0"], Value::Null);
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.verdict.used[0]");
    // Sections in id order: verdict before digests is out of order.
    let mut v = verify_shard().to_json();
    if let Value::Object(m) = &mut v
        && let Some((_, Value::Object(s))) = m.iter_mut().find(|(k, _)| k == "sections")
    {
        s.swap(3, 4);
    }
    assert_eq!(Shard::from_json(&v).unwrap_err().path, "sections.digests");
}

// spec/model.md §5 rule 2, §8: sections 19 and 21 are known (and 17, 24,
// 25 and 26: `tests/edits.rs`, `known_sections_of_m2`); 16, 18 and 20 stay
// reserved, UnknownSection.
#[test]
fn reserved_ids_still_unknown() {
    for id in [16u64, 18, 20] {
        let mut sections = raw_sections(&map_shard());
        sections.push((id, Vec::new()));
        sections.sort_by_key(|s| s.0);
        let bytes = sealed(&sections);
        assert_eq!(error(Shard::from_bytes(&bytes)).0, ErrorKind::UnknownSection, "{id}");
        assert_eq!(error(dump(&bytes)).0, ErrorKind::UnknownSection);
    }
    assert!(Shard::from_bytes(&map_shard_bytes()).unwrap().digests.is_some());
    assert!(Shard::from_bytes(&verify_shard_bytes()).unwrap().verdict.is_some());
}

// ---------------------------------------------------------------------------
// The sections of M2 in a verify shard: binding (24), rule_use (25) and
// spellings (26) (spec/model.md §8, spec/verify.md §9.1, §10.4; WP-060).

fn binding() -> Binding {
    Binding { overlay: Some(d(0x81)), declared: d(0x82), pre: 3 }
}

fn rule_use() -> RuleUse {
    RuleUse {
        rules: d(0x83),
        uses: vec![
            Use { rule: 0, via: 2, direct: 0 },
            Use { rule: 1, via: 0, direct: 1 },
            Use { rule: 300, via: 1 << 60, direct: 1 << 53 },
        ],
    }
}

fn spellings() -> Spellings {
    Spellings {
        seen: 1000,
        held: vec![Span { first: 0, count: 10 }, Span { first: 11, count: 200 }],
        failed: vec![
            Failed { index: 10, found: Some(d(0x84)), name: "::old::T".to_owned() },
            Failed { index: 500, found: None, name: "f".to_owned() },
        ],
    }
}

fn m2_shard() -> Shard {
    Shard {
        binding: Some(binding()),
        rule_use: Some(rule_use()),
        spellings: Some(spellings()),
        ..sample_verify_shard(verified())
    }
}

/// `rejected`, checking also that the offset is `payload + rel` of
/// section `id` in the test's encoding.
fn rejected_at(s: &Shard, path: &str, id: u64, rel: usize) -> String {
    let at = rejected(s, path);
    assert_eq!(at, payload_at(&raw(s), id) + rel, "offset of {path}");
    s.validate().unwrap_err()
}

// spec/model.md §8, §10: the three sections round-trip, alone and
// together, with values at and above 2^53; JSON members in field order.
#[test]
fn m2_sections_round_trip() {
    round_trip(&m2_shard());
    round_trip(&Shard {
        binding: Some(Binding { overlay: None, pre: u64::MAX, ..binding() }),
        ..sample_verify_shard(verified())
    });
    round_trip(&Shard {
        rule_use: Some(RuleUse { uses: Vec::new(), ..rule_use() }),
        ..sample_verify_shard(verified())
    });
    round_trip(&Shard {
        spellings: Some(Spellings { seen: 0, held: Vec::new(), failed: Vec::new() }),
        ..sample_verify_shard(verified())
    });
    // The largest span and a span ending at 2^64.
    let mut x = spellings();
    x.held.push(Span { first: 1 << 60, count: u64::MAX - (1 << 60) + 1 });
    x.seen = u64::MAX;
    round_trip(&Shard { spellings: Some(x), ..sample_verify_shard(verified()) });
    // A binding in a diagnosed shard with no digests or verdict.
    round_trip(&Shard {
        outcome: Outcome {
            status: Status::Diagnosed,
            errors: 1,
            warnings: 0,
            message: "e".to_owned(),
        },
        digests: None,
        verdict: None,
        ..Shard { binding: Some(binding()), ..sample_verify_shard(verified()) }
    });
    let v = m2_shard().to_json();
    let names = |v: &Value| -> Vec<String> {
        v.as_object().unwrap().iter().map(|(k, _)| k.clone()).collect()
    };
    let sections = v.get("sections").unwrap();
    assert_eq!(
        names(sections),
        [
            "header",
            "outcome",
            "inputs",
            "counters",
            "digests",
            "verdict",
            "binding",
            "rule_use",
            "spellings"
        ]
    );
    assert_eq!(names(sections.get("binding").unwrap()), ["overlay", "declared", "pre"]);
    assert_eq!(names(sections.get("rule_use").unwrap()), ["rules", "uses"]);
    let u = &sections.get("rule_use").unwrap().get("uses").unwrap().as_array().unwrap()[2];
    assert_eq!(names(u), ["rule", "via", "direct"]);
    assert_eq!(u.get("via").unwrap().as_str(), Some("1152921504606846976"));
    let sp = sections.get("spellings").unwrap();
    assert_eq!(names(sp), ["seen", "held", "failed"]);
    assert_eq!(names(&sp.get("held").unwrap().as_array().unwrap()[0]), ["first", "count"]);
    assert_eq!(
        names(&sp.get("failed").unwrap().as_array().unwrap()[0]),
        ["index", "found", "name"]
    );
    for (path, new, at) in [
        (&["sections", "binding", "overlay"][..], "0", "sections.binding.overlay"),
        (&["sections", "rule_use", "uses", "1", "via"][..], "-1", "sections.rule_use.uses[1].via"),
        (
            &["sections", "spellings", "held", "0"][..],
            "{\"first\": 0}",
            "sections.spellings.held[0].count",
        ),
        (
            &["sections", "spellings", "failed", "1", "name"][..],
            "{\"hex\": \"ff\"}",
            "sections.spellings.failed[1].name",
        ),
    ] {
        let mut v = m2_shard().to_json();
        set(&mut v, path, parse(new));
        assert_eq!(Shard::from_json(&v).unwrap_err().path, at, "{at}");
    }
}

// spec/model.md §5 rule 2, §8: 17, 24, 25 and 26 are known sections; their
// opt tags are Malformed at their first byte.
#[test]
fn known_sections_of_m2() {
    let bytes = m2_shard().to_bytes();
    for id in [24, 25, 26] {
        payload_at(&bytes, id);
    }
    let b = payload_at(&bytes, 24);
    assert_eq!(error(Shard::from_bytes(&patch(&bytes, b, 1, 2))), (ErrorKind::Malformed, b));
    let f = payload_at(&bytes, 26) + raw_spellings(&spellings()).3[0] + 1; // after the index 10
    assert_eq!(error(Shard::from_bytes(&patch(&bytes, f, 1, 2))), (ErrorKind::Malformed, f));
    // An empty edits section in a map shard.
    let mut s = sample_map_shard(sample_digests(&[]));
    s.edits = Some(Edits {
        scheme: slip_model::edits::SCHEME,
        transform: d(0x85),
        failure: None,
        files: Vec::new(),
        subjects: Vec::new(),
        links: Vec::new(),
        claims: Vec::new(),
        groups: Vec::new(),
        reports: Vec::new(),
        changed: Vec::new(),
        renamed: Vec::new(),
    });
    round_trip(&s);
}

// spec/model.md §8: binding only in a verify shard whose status is not
// failed; rule_use and spellings only in a verify shard whose status is ok.
#[test]
fn m2_sections_phase_and_status() {
    let only = |b: bool, r: bool, sp: bool| Shard {
        binding: b.then(binding),
        rule_use: r.then(rule_use),
        spellings: sp.then(spellings),
        ..sample_verify_shard(verified())
    };
    for (s, path, id) in [
        (only(true, false, false), "sections.binding", 24),
        (only(false, true, false), "sections.rule_use", 25),
        (only(false, false, true), "sections.spellings", 26),
    ] {
        // A map shard.
        let mut m = Shard { phase: Phase::Map, verdict: None, ..s.clone() };
        assert!(rejected_at(&m, path, id, 0).contains("phase is not verify"));
        // A failed verify shard (which can have no digests).
        m = Shard {
            outcome: Outcome {
                status: Status::Failed,
                errors: 1,
                warnings: 0,
                message: "x".to_owned(),
            },
            digests: None,
            verdict: None,
            inputs: None,
            counters: Vec::new(),
            ..s.clone()
        };
        let what = rejected_at(&m, path, id, 0);
        assert!(
            what.contains(if id == 24 { "status is failed" } else { "status is not ok" }),
            "{what}"
        );
        // Diagnosed: only the binding is allowed.
        m.outcome.status = Status::Diagnosed;
        if id == 24 {
            m.validate().unwrap();
        } else {
            assert!(rejected_at(&m, path, id, 0).contains("status is not ok"));
        }
    }
    // In id order: the binding's phase before the rule_use's.
    let s = Shard { phase: Phase::Map, verdict: None, ..m2_shard() };
    rejected_at(&s, "sections.binding", 24, 0);
}

// spec/verify.md §9.1: uses strictly ascending by rule; no use with via
// and direct both 0. Each rule over the whole list.
#[test]
fn rule_use_invariants() {
    let with =
        |uses: Vec<Use>| Shard { rule_use: Some(RuleUse { uses, ..rule_use() }), ..m2_shard() };
    let u = |rule, via, direct| Use { rule, via, direct };
    for uses in [
        vec![u(1, 1, 0), u(0, 1, 0)],
        vec![u(1, 1, 0), u(1, 0, 1)],
        vec![u(0, 1, 0), u(5, 1, 1), u(5, 1, 1)],
    ] {
        let n = uses.len() - 1;
        let s = with(uses);
        let rel = raw_rule_use(s.rule_use.as_ref().unwrap()).1[n];
        assert!(
            rejected_at(&s, &format!("sections.rule_use.uses[{n}]"), 25, rel).contains("ascending")
        );
    }
    let s = with(vec![u(0, 1, 0), u(1, 0, 0)]);
    let rel = raw_rule_use(s.rule_use.as_ref().unwrap()).1[1];
    assert!(rejected_at(&s, "sections.rule_use.uses[1]", 25, rel).contains("both 0"));
    // The order rule over the whole list first.
    let s = with(vec![u(0, 0, 0), u(2, 1, 0), u(1, 1, 0)]);
    let rel = raw_rule_use(s.rule_use.as_ref().unwrap()).1[2];
    rejected_at(&s, "sections.rule_use.uses[2]", 25, rel);
}

// spec/verify.md §10.4: no span of count 0 or running past 2^64 - 1;
// spans ascending, not overlapping (an index held twice) or touching;
// failed strictly ascending; no index both held and failed; no more
// indices than seen.
#[test]
fn spellings_invariants() {
    let with = |f: fn(&mut Spellings)| {
        let mut x = spellings();
        f(&mut x);
        Shard { spellings: Some(x), ..m2_shard() }
    };
    let check = |s: Shard, path: &str, which: usize, i: usize, what: &str| {
        let r = raw_spellings(s.spellings.as_ref().unwrap());
        let rel = match which {
            0 => 0,
            1 => r.1[i],
            2 => r.2[i],
            _ => r.3[i],
        };
        let got = rejected_at(&s, path, 26, rel);
        assert!(got.contains(what), "{path}: {got}");
    };
    check(with(|x| x.held[1].count = 0), "sections.spellings.held[1].count", 2, 1, "count of 0");
    check(
        with(|x| x.held.push(Span { first: u64::MAX, count: 2 })),
        "sections.spellings.held[2].count",
        2,
        2,
        "past the largest",
    );
    // An index held twice: overlapping spans, or the same span twice.
    check(with(|x| x.held[1].first = 9), "sections.spellings.held[1]", 1, 1, "touch");
    check(with(|x| x.held[1] = x.held[0]), "sections.spellings.held[1]", 1, 1, "touch");
    // Touching spans; spans out of order.
    check(with(|x| x.held[1].first = 10), "sections.spellings.held[1]", 1, 1, "touch");
    check(with(|x| x.held.swap(0, 1)), "sections.spellings.held[1]", 1, 1, "touch");
    // A span after the one ending at 2^64.
    check(
        with(|x| {
            x.held.push(Span { first: u64::MAX, count: 1 });
            x.held.push(Span { first: 5, count: 1 });
        }),
        "sections.spellings.held[3]",
        1,
        3,
        "touch",
    );
    check(with(|x| x.failed.swap(0, 1)), "sections.spellings.failed[1]", 3, 1, "ascending");
    check(with(|x| x.failed[1].index = 10), "sections.spellings.failed[1]", 3, 1, "ascending");
    // An index in both lists: inside a span, at its first and last index.
    for index in [11, 5, 0, 210] {
        let mut x = spellings();
        x.failed[0].index = index;
        x.failed[0].name = format!("{index}");
        let s = Shard { spellings: Some(x), ..m2_shard() };
        check(s, "sections.spellings.failed[0]", 3, 0, "both held and failed");
    }
    check(with(|x| x.seen = 211), "sections.spellings.seen", 0, 0, "than seen");
    // Exactly seen is allowed; overflow of the total is not wrapped.
    let mut x = spellings();
    x.seen = 212;
    Shard { spellings: Some(x), ..m2_shard() }.validate().unwrap();
    check(
        with(|x| {
            x.held = vec![Span { first: 0, count: u64::MAX }];
            x.failed = vec![Failed { index: u64::MAX, found: None, name: String::new() }];
            x.seen = u64::MAX;
        }),
        "sections.spellings.seen",
        0,
        0,
        "than seen",
    );
    // The order of the rules: a count of 0 before an unordered failed.
    check(
        with(|x| {
            x.failed.swap(0, 1);
            x.held[1].count = 0;
        }),
        "sections.spellings.held[1].count",
        2,
        1,
        "count of 0",
    );
    check(
        with(|x| {
            x.seen = 0;
            x.failed[0].index = 5;
        }),
        "sections.spellings.failed[0]",
        3,
        0,
        "both",
    );
}
