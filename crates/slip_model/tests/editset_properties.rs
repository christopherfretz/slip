//! Properties of the edit set, overlay, result and transform run record
//! readers over corrupted and generated files (`spec/model.md` §1 rule 6,
//! §2.1), of the geometry of `spec/edits.md` §5.4, and of the edit set's
//! crossing checks against a check of every pair (§6.1). Work package
//! WP-061. A fixed seed: the same cases every run.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use slip_model::container::TRAILER_LEN;
use slip_model::edits::{Piece, Place, Reason, tiers};
use slip_model::editset::{
    Cause, Edit, EditSet, Refusal, SetState, cross, properly_inside, share_byte, within,
};
use slip_model::formats::{content_id, dump, undump};
use slip_model::hash::content_digest;
use slip_model::json;
use slip_model::overlay::{Content, Obligation, Overlay, OverlayFile};
use slip_model::result::TransformResult;
use slip_model::transform_run::TransformRun;
use slip_model::{Counter, Digest, FileRef};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

// ---------------------------------------------------------------------------
// A generator (a linear congruential one: no dependency)

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

    fn index(&mut self, n: usize) -> usize {
        usize::try_from(self.below(n as u64)).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Corrupted files

/// The bytes of each golden rendering of the four kinds (`tests/editset.rs`).
fn goldens() -> Vec<Vec<u8>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    [
        "editset.json",
        "editset-refused.json",
        "overlay.json",
        "result.json",
        "result-refused.json",
        "transform-run.json",
    ]
    .iter()
    .map(|name| {
        let text = fs::read_to_string(dir.join(name)).unwrap();
        undump(&json::parse(text.as_bytes()).unwrap()).unwrap()
    })
    .collect()
}

fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() >= TRAILER_LEN {
        let body = bytes.len() - TRAILER_LEN;
        let t = content_digest(&bytes[..body]);
        bytes[body..].copy_from_slice(t.as_bytes());
    }
    bytes
}

/// No reader panics on `bytes`; one that accepts them writes them back as
/// read (the canonical form), and the value is valid and survives JSON.
fn survives(bytes: &[u8]) {
    fn one<T: PartialEq + std::fmt::Debug>(
        bytes: &[u8],
        read: fn(&[u8]) -> Result<T, slip_model::wire::Error>,
        write: fn(&T) -> Vec<u8>,
        check: fn(&T) -> Result<(), String>,
        json: fn(&T) -> Option<T>,
    ) {
        let r = catch_unwind(AssertUnwindSafe(|| read(bytes)));
        let Ok(r) = r else { panic!("from_bytes panicked") };
        if let Ok(x) = r {
            check(&x).unwrap();
            assert_eq!(write(&x), bytes, "an accepted file is written back as read");
            assert_eq!(json(&x).as_ref(), Some(&x));
        }
    }
    one(bytes, EditSet::from_bytes, EditSet::to_bytes, EditSet::validate, |x| {
        EditSet::from_json(&x.to_json()).ok()
    });
    one(bytes, Overlay::from_bytes, Overlay::to_bytes, Overlay::validate, |x| {
        Overlay::from_json(&x.to_json()).ok()
    });
    one(
        bytes,
        TransformResult::from_bytes,
        TransformResult::to_bytes,
        TransformResult::validate,
        |x| TransformResult::from_json(&x.to_json()).ok(),
    );
    one(bytes, TransformRun::from_bytes, TransformRun::to_bytes, TransformRun::validate, |x| {
        TransformRun::from_json(&x.to_json()).ok()
    });
    let r = catch_unwind(AssertUnwindSafe(|| content_id(bytes)));
    assert!(r.is_ok(), "content_id panicked");
}

// spec/model.md §1 rule 6, §2.1: every truncation of each golden, and
// single-byte corruptions of every byte (resealed, so that the trailer
// does not hide them), make no reader panic, and what is accepted is in
// canonical form.
#[test]
fn corrupted_goldens() {
    // The refused edit set is the accepted one less its edits: skipped.
    for (i, bytes) in goldens().into_iter().enumerate() {
        if i == 1 {
            continue;
        }
        survives(&bytes);
        for n in 0..bytes.len() {
            survives(&reseal(bytes[..n].to_vec()));
        }
        // Unsealed: refused by the trailer, whatever the cut.
        survives(&bytes[..bytes.len() - 1]);
        for at in 0..bytes.len() - TRAILER_LEN {
            for v in [0x00, 0xff, bytes[at] ^ 0x01, bytes[at] ^ 0x80] {
                if v == bytes[at] {
                    continue;
                }
                let mut b = bytes.clone();
                b[at] = v;
                survives(&reseal(b));
            }
        }
    }
}

// spec/model.md §2.1: a count larger than the bytes left is refused, not
// preallocated: an edit set whose `files` count is 2^62.
#[test]
fn huge_counts() {
    let bytes = goldens().remove(0);
    // Magic, kind, version, section id 1 and a two-byte length; then
    // scheme, plan, round, transform, basis and state.
    let state = 19 + 1 + 32 + 1 + 32 + 32;
    assert_eq!(bytes[state + 1], 3, "files count of the sample");
    let mut b = bytes[..state + 1].to_vec();
    b.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x3f]);
    b.extend_from_slice(&bytes[state + 2..]);
    let b = reseal(b);
    assert!(EditSet::from_bytes(&b).is_err());
    survives(&b);
}

// ---------------------------------------------------------------------------
// Generated values

fn gen_set(r: &mut Lcg) -> EditSet {
    let n = 1 + r.below(3);
    let files: Vec<FileRef> = (0..n)
        .map(|i| FileRef {
            path: format!("/g/f{i}").into_bytes(),
            size: 16 + r.below(48),
            content: d(u8::try_from(i).unwrap()),
        })
        .collect();
    let mut edits = Vec::new();
    for _ in 0..r.below(6) {
        let file = r.below(n);
        let lo = r.below(16);
        let hi = lo + 1 + r.below(12);
        let mut pieces = Vec::new();
        if r.below(2) == 0 {
            pieces.push(Piece::Literal(b"z".to_vec()));
        }
        if r.below(2) == 0 {
            let lo = r.below(16);
            pieces.push(Piece::Copy(Place { file: r.below(n), lo, hi: lo + 1 + r.below(6) }));
        }
        if r.below(3) == 0 {
            pieces.push(Piece::Spelled { key: d(0x10), text: b"g".to_vec() });
        }
        let observers = 1 + r.below(3);
        edits.push(Edit {
            file,
            lo,
            hi,
            pieces,
            subjects: if r.below(2) == 0 { vec![d(0x10)] } else { Vec::new() },
            tiers: tiers::T0,
            macro_site: None,
            notes: Vec::new(),
            observers,
            votes: observers,
            first: (0..r.below(observers + 1))
                .map(|i| d(0xa0 + u8::try_from(i).unwrap()))
                .collect(),
        });
    }
    edits.sort_by_key(|e| (e.file, e.lo, e.hi));
    edits.dedup_by_key(|e| (e.file, e.lo, e.hi));
    let mut refusals: Vec<Refusal> = (0..r.below(3))
        .map(|_| {
            let cause = [Cause::MixedSet, Cause::Contested, Cause::SiteRefused, Cause::NoObserver]
                [r.index(4)];
            let file = (r.below(2) == 0).then(|| r.below(n));
            let (lo, hi) = if file.is_some() { (r.below(8), 8 + r.below(8)) } else { (0, 0) };
            Refusal {
                cause,
                file,
                lo,
                hi,
                reason: if cause == Cause::SiteRefused { Reason::Pasted } else { Reason::None },
                subject: (r.below(2) == 0).then(|| d(0x10)),
                macro_site: None,
                other: None,
                observers: 0,
                first: Vec::new(),
            }
        })
        .collect();
    refusals.sort_by(Refusal::cmp_key);
    refusals.dedup_by(|a, b| a.cmp_key(b).is_eq());
    let forced = refusals.iter().any(|x| x.cause == Cause::NoObserver);
    let state = if edits.is_empty() || forced || r.below(8) == 0 {
        SetState::Refused
    } else {
        SetState::Accepted
    };
    if state == SetState::Refused && r.below(4) != 0 {
        edits.clear();
    }
    let groups = if edits.len() >= 2 && r.below(2) == 0 {
        vec![vec![0, edits.len() as u64 - 1]]
    } else {
        Vec::new()
    };
    EditSet {
        scheme: 1,
        plan: d(1),
        round: r.below(3),
        transform: d(2),
        basis: d(3),
        state,
        files,
        subjects: Vec::new(),
        rules: Vec::new(),
        edits,
        groups,
        changed: Vec::new(),
        kept: if r.below(2) == 0 {
            vec![Place { file: 0, lo: 0, hi: r.below(20) }]
        } else {
            Vec::new()
        },
        unresolved: Vec::new(),
        refusals,
        reports: Vec::new(),
        failures: Vec::new(),
        unparsed: if r.below(2) == 0 { vec![d(0xb0)] } else { Vec::new() },
        counters: if r.below(2) == 0 {
            vec![Counter { name: "reduce.edits".to_owned(), value: r.below(9) }]
        } else {
            Vec::new()
        },
    }
}

fn gen_overlay(r: &mut Lcg) -> Overlay {
    let n = r.below(4);
    let files: Vec<OverlayFile> = (0..n)
        .map(|i| {
            let pre = Content { size: 8 + r.below(8), content: d(u8::try_from(i).unwrap()) };
            let post = if r.below(3) == 0 {
                pre
            } else {
                Content { size: 4 + r.below(12), content: d(0x40) }
            };
            OverlayFile { path: format!("/o/{i}").into_bytes(), pre, post }
        })
        .collect();
    let mut spellings: Vec<Obligation> = (0..r.below(6))
        .map(|_| {
            let lo = r.below(12);
            Obligation {
                file: r.below(n + 1),
                lo,
                hi: lo + r.below(4),
                key: d(0x10 + u8::try_from(r.below(2)).unwrap()),
            }
        })
        .collect();
    if r.below(4) != 0 {
        spellings.sort();
        spellings.dedup();
    }
    Overlay { edits: d(4), files, spellings }
}

// spec/edits.md §6.1, §7.1, spec/model.md §10: a generated value that
// validates survives both forms; one that does not is refused by
// `from_json` and `undump` at the path and with the message `validate`
// gives. The geometry rules of the generated edit sets are broken often.
#[test]
fn generated_values() {
    let mut r = Lcg(0x0061_0061);
    let (mut valid, mut invalid) = (0, 0);
    for _ in 0..3000 {
        let x = gen_set(&mut r);
        match x.validate() {
            Ok(()) => {
                valid += 1;
                let b = x.to_bytes();
                assert_eq!(EditSet::from_bytes(&b).unwrap(), x);
                assert_eq!(EditSet::from_json(&x.to_json()).unwrap(), x);
                assert_eq!(undump(&json::parse(dump(&b).unwrap().as_bytes()).unwrap()).unwrap(), b);
            }
            Err(v) => {
                invalid += 1;
                let e = EditSet::from_json(&x.to_json()).unwrap_err();
                assert_eq!(v, format!("{}: {}", e.path, e.message));
                assert_eq!(undump(&x.to_json()).unwrap_err(), e);
            }
        }
        let o = gen_overlay(&mut r);
        match o.validate() {
            Ok(()) => {
                let b = o.to_bytes();
                assert_eq!(Overlay::from_bytes(&b).unwrap(), o);
                assert_eq!(Overlay::from_json(&o.to_json()).unwrap(), o);
                survives(&b);
            }
            Err(v) => {
                let e = Overlay::from_json(&o.to_json()).unwrap_err();
                assert_eq!(v, format!("{}: {}", e.path, e.message));
            }
        }
    }
    assert!(valid > 300 && invalid > 300, "valid {valid}, invalid {invalid}");
}

// ---------------------------------------------------------------------------
// Geometry

// spec/edits.md §5.4, spec/landed/edits/WP-061.md §1: for any two ranges,
// empty and reversed ones included, exactly one of: disjoint, one within
// the other, crossing. The functions agree with their definitions over
// bytes.
#[test]
fn geometry_is_consistent() {
    let bytes = |a: (u64, u64)| (a.0..a.1).collect::<Vec<_>>();
    for alo in 0..9 {
        for ahi in 0..9 {
            for blo in 0..9 {
                for bhi in 0..9 {
                    let (a, b) = ((alo, ahi), (blo, bhi));
                    let disjoint = !share_byte(a, b);
                    let nested = within(a, b) || within(b, a);
                    let crosses = cross(a, b);
                    assert_eq!(
                        u8::from(disjoint) + u8::from(nested) + u8::from(crosses),
                        1,
                        "{a:?} {b:?}"
                    );
                    let (x, y) = (bytes(a), bytes(b));
                    assert_eq!(share_byte(a, b), x.iter().any(|i| y.contains(i)), "{a:?} {b:?}");
                    assert_eq!(
                        within(a, b),
                        !x.is_empty() && x.iter().all(|i| y.contains(i)),
                        "{a:?} {b:?}"
                    );
                    assert_eq!(properly_inside(a, b), within(a, b) && a != b);
                    assert_eq!(share_byte(a, b), share_byte(b, a));
                    assert_eq!(cross(a, b), cross(b, a));
                    if within(a, b) && within(b, a) {
                        assert_eq!(a, b);
                    }
                }
            }
        }
    }
}

/// The first edit of `x` that breaks row 8 of §6.1, by a check of every
/// pair: ascending, not empty, crossing no earlier edit of its file,
/// crossing no non-empty copied range.
fn brute_geometry(x: &EditSet) -> Option<(usize, &'static str)> {
    let copies: Vec<(u64, (u64, u64))> = x
        .edits
        .iter()
        .flat_map(|e| e.pieces.iter())
        .filter_map(|p| match p {
            Piece::Copy(c) => Some((c.file, (c.lo, c.hi))),
            _ => None,
        })
        .collect();
    for (i, e) in x.edits.iter().enumerate() {
        if i > 0 {
            let p = &x.edits[i - 1];
            if (p.file, p.lo, p.hi) >= (e.file, e.lo, e.hi) {
                return Some((i, "edits are not strictly ascending by (file, lo, hi)"));
            }
        }
        if e.lo >= e.hi {
            return Some((i, "an edit's range is empty"));
        }
        if x.edits[..i].iter().any(|p| p.file == e.file && cross((p.lo, p.hi), (e.lo, e.hi))) {
            return Some((i, "an edit crosses another edit"));
        }
        if copies.iter().any(|&(f, c)| f == e.file && cross(c, (e.lo, e.hi))) {
            return Some((i, "an edit crosses a range an edit copies"));
        }
    }
    None
}

// spec/edits.md §6.1 row 8: the stack and segment-tree checks report what
// a check of every pair reports, the least index and its rule, over many
// dense generated sets (unsorted ones and empty ranges included).
#[test]
fn crossing_checks_agree_with_every_pair() {
    let mut r = Lcg(0x0061_0054);
    let mut found = [0usize; 5];
    for _ in 0..20_000 {
        let n_edits = 1 + r.index(9);
        let mut edits: Vec<Edit> = (0..n_edits)
            .map(|_| {
                let lo = r.below(24);
                let hi = if r.below(30) == 0 { lo } else { lo + 1 + r.below(10) };
                let pieces = match r.below(4) {
                    0 => Vec::new(),
                    1 => vec![Piece::Literal(b"q".to_vec())],
                    _ => {
                        let lo = r.below(30);
                        vec![Piece::Copy(Place { file: r.below(2), lo, hi: lo + 1 + r.below(8) })]
                    }
                };
                Edit {
                    file: r.below(2),
                    lo,
                    hi,
                    pieces,
                    subjects: Vec::new(),
                    tiers: tiers::T0,
                    macro_site: None,
                    notes: Vec::new(),
                    observers: 1,
                    votes: 1,
                    first: Vec::new(),
                }
            })
            .collect();
        if r.below(20) != 0 {
            edits.sort_by_key(|e| (e.file, e.lo, e.hi));
            if r.below(10) != 0 {
                edits.dedup_by_key(|e| (e.file, e.lo, e.hi));
            }
        }
        let x = EditSet {
            scheme: 1,
            plan: d(1),
            round: 0,
            transform: d(2),
            basis: d(3),
            state: SetState::Accepted,
            files: vec![
                FileRef { path: b"/g/a".to_vec(), size: 64, content: d(4) },
                FileRef { path: b"/g/b".to_vec(), size: 64, content: d(5) },
            ],
            subjects: Vec::new(),
            rules: Vec::new(),
            edits,
            groups: Vec::new(),
            changed: Vec::new(),
            kept: Vec::new(),
            unresolved: Vec::new(),
            refusals: Vec::new(),
            reports: Vec::new(),
            failures: Vec::new(),
            unparsed: Vec::new(),
            counters: Vec::new(),
        };
        let want = brute_geometry(&x);
        found[match want {
            None => 0,
            Some((_, "edits are not strictly ascending by (file, lo, hi)")) => 1,
            Some((_, "an edit's range is empty")) => 2,
            Some((_, "an edit crosses another edit")) => 3,
            Some(_) => 4,
        }] += 1;
        let got = x.validate();
        match want {
            None => assert_eq!(got, Ok(()), "{:?}", x.edits),
            Some((i, what)) => {
                assert_eq!(got, Err(format!("sections.set.edits[{i}]: {what}")), "{:?}", x.edits);
            }
        }
    }
    assert!(found.iter().all(|&n| n > 100), "{found:?}");
}
