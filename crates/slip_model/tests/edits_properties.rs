//! Properties of `Edits::normalise` (`spec/edits.md` §4.2–§4.4) over
//! generated sections, and of the readers of `edits` and `TRFM` over
//! corrupted files (`spec/model.md` §1 rule 6, §2.1). Work package WP-060,
//! from its review's probe. A fixed seed: the same cases every run.

use std::panic::{AssertUnwindSafe, catch_unwind};

use slip_model::container::{TRAILER_LEN, Writer};
use slip_model::edits::{
    Changed, Claim, Edits, Failure, FailureKind, HostRefusal, MacroSite, Piece, Place, Reason,
    Report, Role, SCHEME, Severity, Stance, Subject, tiers,
};
use slip_model::hash::content_digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule};
use slip_model::transform::{Ownership, Rename, TargetKind, Transform, TransformKind};
use slip_model::{Digest, FileRef, Outcome, Phase, Shard, Status};

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn file(path: &[u8], size: u64, n: u8) -> FileRef {
    FileRef { path: path.to_vec(), size, content: d(n) }
}

fn map_shard(edits: Edits) -> Shard {
    Shard {
        observer: d(0x33),
        phase: Phase::Map,
        round: 0,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: Vec::new(),
        digests: None,
        verdict: None,
        edits: Some(edits),
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
    }
}

fn empty() -> Edits {
    Edits {
        scheme: SCHEME,
        transform: d(0x44),
        failure: None,
        files: Vec::new(),
        subjects: Vec::new(),
        links: Vec::new(),
        claims: Vec::new(),
        groups: Vec::new(),
        reports: Vec::new(),
        changed: Vec::new(),
        renamed: Vec::new(),
    }
}

fn norm(mut x: Edits) -> Edits {
    x.normalise();
    x
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

    fn subset<T: Clone>(&mut self, of: &[T]) -> Vec<T> {
        of.iter().filter(|_| self.below(2) == 0).cloned().collect()
    }
}

/// A claim of a few sites and values, so that derivations often agree in
/// the sort key and differ in the rest.
fn gen_claim(r: &mut Lcg, files: u64) -> Claim {
    let stance = [Stance::Replace, Stance::Keep, Stance::Refuse, Stance::Unresolved][r.index(4)];
    let lo = [10u64, 20][r.index(2)];
    let pieces = if stance == Stance::Replace {
        match r.below(4) {
            0 => vec![],
            1 => vec![Piece::Literal(b"g".to_vec())],
            2 => vec![Piece::Spelled {
                key: d(1 + u8::try_from(r.below(2)).unwrap()),
                text: b"g".to_vec(),
            }],
            _ => vec![Piece::Copy(Place { file: r.below(files), lo: 0, hi: 1 })],
        }
    } else {
        vec![]
    };
    let reason = if stance == Stance::Refuse {
        [
            Reason::MacroBody,
            Reason::Pasted,
            Reason::NotOwned,
            Reason::MixedInstances,
            Reason::Directive,
        ][r.index(5)]
    } else {
        Reason::None
    };
    let t = if stance == Stance::Refuse { 1 + r.below(31) } else { 1 + r.below(3) };
    let macro_site = if t == tiers::T0 || r.below(3) == 0 {
        None
    } else {
        Some(MacroSite {
            name_start: r.below(3),
            name: [b"A".to_vec(), b"B".to_vec()][r.index(2)].clone(),
        })
    };
    Claim {
        file: r.below(files),
        lo,
        hi: lo + 1,
        stance,
        pieces,
        subjects: r.subset(&[d(1), d(2), d(3)]),
        reason,
        tiers: t,
        macro_site,
        votes: 1 + r.below(3),
        notes: r.subset(&[b"a".to_vec(), b"b".to_vec(), b"c\xff".to_vec()]),
    }
}

/// A section in no particular order: files sorted (the claims' indices
/// name them), claims, groups of replace claims, reports and `changed`
/// with repeats.
fn gen_edits(r: &mut Lcg) -> Edits {
    let n = 1 + r.below(3);
    let files: Vec<FileRef> = (0..n)
        .map(|i| file(format!("/s/f{i}").as_bytes(), 100, u8::try_from(i).unwrap()))
        .collect();
    let claims: Vec<Claim> = (0..r.below(8)).map(|_| gen_claim(r, n)).collect();
    let replaces: Vec<u64> = (0..claims.len() as u64)
        .filter(|&i| claims[usize::try_from(i).unwrap()].stance == Stance::Replace)
        .collect();
    let mut groups = Vec::new();
    for _ in 0..r.below(3) {
        let g = r.subset(&replaces);
        if g.len() >= 2 {
            groups.push(g);
        }
    }
    let reports = (0..r.below(4))
        .map(|_| Report {
            place: (r.below(2) == 0).then(|| Place { file: r.below(n), lo: 1, hi: 2 }),
            severity: [Severity::Note, Severity::Review, Severity::Error][r.index(3)],
            message: vec![b'a' + u8::try_from(r.below(2)).unwrap()],
            attached: r.below(2) == 0,
        })
        .collect();
    let changed = (0..r.below(3))
        .map(|_| Changed {
            key: d(u8::try_from(r.below(2)).unwrap()),
            place: Place { file: r.below(n), lo: 1, hi: 2 },
            reason: vec![b'r'],
        })
        .collect();
    Edits { files, claims, groups, reports, changed, ..empty() }
}

/// `x` with its files and claims permuted (every index following), and
/// its groups, members, reports and `changed` reversed.
fn shuffle(x: &Edits, r: &mut Lcg) -> Edits {
    let permutation = |r: &mut Lcg, n: usize| {
        let mut p: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            p.swap(i, r.index(i + 1));
        }
        let mut inverse = vec![0u64; n];
        for (new, &old) in p.iter().enumerate() {
            inverse[old] = new as u64;
        }
        (p, inverse)
    };
    let mut y = x.clone();
    let (p, inv) = permutation(r, x.files.len());
    y.files = p.iter().map(|&o| x.files[o].clone()).collect();
    let fm = |f: &mut u64| *f = inv[usize::try_from(*f).unwrap()];
    for c in &mut y.claims {
        fm(&mut c.file);
        for piece in &mut c.pieces {
            if let Piece::Copy(place) = piece {
                fm(&mut place.file);
            }
        }
    }
    for rep in &mut y.reports {
        if let Some(place) = &mut rep.place {
            fm(&mut place.file);
        }
    }
    for c in &mut y.changed {
        fm(&mut c.place.file);
    }
    let (p, inv) = permutation(r, x.claims.len());
    let old = y.claims.clone();
    y.claims = p.iter().map(|&o| old[o].clone()).collect();
    for g in &mut y.groups {
        for m in g.iter_mut() {
            *m = inv[usize::try_from(*m).unwrap()];
        }
        g.reverse();
    }
    y.groups.reverse();
    y.reports.reverse();
    y.changed.reverse();
    y
}

/// The claims split at `k`, each half normalised, then the concatenation
/// normalised; `None` when a group spans the halves.
#[allow(clippy::many_single_char_names, reason = "the whole, its halves a and b, their join c")]
fn split_join(x: &Edits, k: usize) -> Option<Edits> {
    let k64 = k as u64;
    if x.groups.iter().any(|g| g.iter().any(|&m| m < k64) && g.iter().any(|&m| m >= k64)) {
        return None;
    }
    let mut a = x.clone();
    a.claims.truncate(k);
    a.groups.retain(|g| g.iter().all(|&m| m < k64));
    a.reports.clear();
    a.changed.clear();
    let mut b = x.clone();
    b.claims = x.claims[k..].to_vec();
    b.groups = x
        .groups
        .iter()
        .filter(|g| g.iter().all(|&m| m >= k64))
        .map(|g| g.iter().map(|&m| m - k64).collect())
        .collect();
    let (a, b) = (norm(a), norm(b));
    let mut c = a.clone();
    let off = c.claims.len() as u64;
    c.claims.extend(b.claims);
    c.groups.extend(b.groups.iter().map(|g| g.iter().map(|&m| m + off).collect()));
    c.reports = b.reports;
    c.changed = b.changed;
    Some(norm(c))
}

/// §4.3, computed from the derivations themselves: what the one claim of
/// their sort key holds.
fn check_join(case: usize, x: &Edits, n: &Edits) {
    for c in &x.claims {
        let all: Vec<&Claim> = x.claims.iter().filter(|o| o.cmp_key(c).is_eq()).collect();
        let joined: Vec<&Claim> = n.claims.iter().filter(|j| j.cmp_key(c).is_eq()).collect();
        assert_eq!(joined.len(), 1, "case {case}: one claim per sort key");
        let j = joined[0];
        let mut subjects: Vec<Digest> = all.iter().flat_map(|o| o.subjects.clone()).collect();
        subjects.sort();
        subjects.dedup();
        let mut notes: Vec<Vec<u8>> = all.iter().flat_map(|o| o.notes.clone()).collect();
        notes.sort();
        notes.dedup();
        assert_eq!(j.votes, all.iter().map(|o| o.votes).sum::<u64>(), "case {case}: votes");
        assert_eq!(j.tiers, all.iter().fold(0, |t, o| t | o.tiers), "case {case}: tiers");
        assert_eq!(
            j.macro_site,
            all.iter().filter_map(|o| o.macro_site.clone()).min(),
            "case {case}: macro"
        );
        let reason = all.iter().map(|o| o.reason).filter(|r| *r != Reason::None).min();
        assert_eq!(j.reason, reason.unwrap_or(Reason::None), "case {case}: reason");
        assert_eq!(j.subjects, subjects, "case {case}: subjects");
        assert_eq!(j.notes, notes, "case {case}: notes");
    }
}

// §4.2–§4.4: over 5000 generated sections, normalise gives a valid
// section; is idempotent; does not depend on the order of the files,
// claims, groups, reports or `changed`; normalising two halves and then
// their concatenation is normalising the whole; and a joined claim is
// what §4.3 says.
#[test]
fn normalise_properties() {
    let mut r = Lcg(0x0060_0060);
    for case in 0..5000 {
        let x = gen_edits(&mut r);
        let n = norm(x.clone());
        if let Err(e) = map_shard(n.clone()).validate() {
            panic!("case {case}: normalised section is invalid: {e}\n{x:?}");
        }
        assert_eq!(norm(n.clone()), n, "case {case}: idempotent");
        for _ in 0..3 {
            let y = shuffle(&x, &mut r);
            assert_eq!(norm(y.clone()), n, "case {case}: order\n{x:?}\n{y:?}");
        }
        let k = r.index(x.claims.len() + 1);
        if let Some(z) = split_join(&x, k) {
            assert_eq!(z, n, "case {case}: split at {k}");
        }
        check_join(case, &x, &n);
    }
}

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

fn subject(id: u8, display: &str) -> Subject {
    Subject {
        id: d(id),
        rule: None,
        role: Role::Candidate,
        of: Vec::new(),
        flags: 0,
        display: display.to_owned(),
    }
}

// normalise: an input it cannot make valid (two files with one path, two
// subjects with one id, two rules with one match) still has one result,
// whatever its order: the ties are broken by the rest of the record.
#[test]
fn normalise_ties_do_not_depend_on_order() {
    let c = |file: u64, text: &[u8]| Claim {
        file,
        lo: 1,
        hi: 2,
        stance: Stance::Replace,
        pieces: vec![
            Piece::Literal(text.to_vec()),
            Piece::Copy(Place { file: 1 - file, lo: 0, hi: 1 }),
        ],
        subjects: Vec::new(),
        reason: Reason::None,
        tiers: tiers::T0,
        macro_site: None,
        votes: 1,
        notes: Vec::new(),
    };
    let one = Edits {
        files: vec![file(b"/s/a.h", 10, 1), file(b"/s/a.h", 10, 2)],
        claims: vec![c(0, b"x"), c(1, b"y")],
        subjects: vec![subject(5, "b"), subject(5, "a")],
        renamed: vec![rule(b"f", b"h"), rule(b"f", b"g")],
        ..empty()
    };
    let other = Edits {
        files: vec![one.files[1].clone(), one.files[0].clone()],
        claims: vec![c(1, b"x"), c(0, b"y")],
        subjects: vec![subject(5, "a"), subject(5, "b")],
        renamed: vec![rule(b"f", b"g"), rule(b"f", b"h")],
        ..empty()
    };
    let (a, b) = (norm(one), norm(other));
    assert_eq!(a, b);
    let text = |x: &Edits| slip_model::json::write_pretty(&map_shard(x.clone()).to_json()).unwrap();
    assert_eq!(text(&a), text(&b));
    // And the result is the order of the whole record.
    assert_eq!(a.files[0].content, d(1));
    assert_eq!(a.subjects[0].display, "a");
    assert_eq!(a.renamed[0].to_name, Name::Identifier(b"g".to_vec()));
    assert!(map_shard(a).validate().is_err());
}

// ---------------------------------------------------------------------------
// Corrupted files: no reader panics; what one accepts it writes back
// byte for byte.

fn payloads(bytes: &[u8]) -> Vec<(u64, Vec<u8>)> {
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
    let mut out = Vec::new();
    loop {
        let id = uv(bytes, &mut at);
        if id == 0 {
            return out;
        }
        let len = usize::try_from(uv(bytes, &mut at)).unwrap();
        out.push((id, bytes[at..at + len].to_vec()));
        at += len;
    }
}

fn build(kind: [u8; 4], sections: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut w = Writer::new(kind, if kind == *b"SHRD" { Shard::VERSION } else { 1 });
    for (id, p) in sections {
        w.section(*id, p);
    }
    w.finish()
}

fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let body = bytes.len() - TRAILER_LEN;
    let t = content_digest(&bytes[..body]);
    bytes[body..].copy_from_slice(t.as_bytes());
    bytes
}

/// Every truncation of section `id`'s payload, and every byte of it set to
/// a few values, in a file of kind `kind` otherwise as `base`.
fn variants(kind: [u8; 4], base: &[u8], id: u64) -> Vec<Vec<u8>> {
    let sections = payloads(base);
    let k = sections.iter().position(|s| s.0 == id).unwrap();
    let payload = &sections[k].1;
    let mut out = Vec::new();
    let mut with = |p: Vec<u8>| {
        let mut s = sections.clone();
        s[k].1 = p;
        out.push(build(kind, &s));
    };
    for n in 0..payload.len() {
        with(payload[..n].to_vec());
    }
    for i in 0..payload.len() {
        for v in [0x00, 0x01, 0x02, 0x7f, 0x80, 0xff, payload[i] ^ 1, payload[i].wrapping_add(1)] {
            if v != payload[i] {
                let mut p = payload.clone();
                p[i] = v;
                with(p);
            }
        }
        let mut p = payload.clone();
        p.insert(i, 0x80);
        with(p);
    }
    for i in 0..base.len() - TRAILER_LEN {
        let mut b = base.to_vec();
        b[i] ^= 0x01;
        out.push(reseal(b));
    }
    out
}

fn shard_survives(bytes: &[u8]) {
    let r = catch_unwind(AssertUnwindSafe(|| Shard::from_bytes(bytes)));
    let Ok(r) = r else { panic!("from_bytes panicked") };
    if let Ok(s) = r {
        s.validate().unwrap();
        assert_eq!(s.to_bytes(), bytes, "an accepted shard is written back as read");
        assert_eq!(Shard::from_json(&s.to_json()).unwrap(), s);
    }
}

// spec/model.md §1 rule 6, §2.1: corrupting the edits section of a few
// generated shards and of a failed run, and every byte of the file, makes
// no reader panic.
#[test]
fn corrupted_edits_sections() {
    let mut r = Lcg(0x0060_0061);
    let mut bases: Vec<Edits> =
        (0..40).map(|_| norm(gen_edits(&mut r))).filter(|x| x.claims.len() >= 2).take(3).collect();
    bases.push(Edits {
        failure: Some(Failure {
            kind: FailureKind::Uncaught,
            message: "m".to_owned(),
            place: Some(slip_model::edits::SourcePlace {
                file: b"/t".to_vec(),
                line: 3,
                column: 7,
            }),
            rendered: "r".to_owned(),
            refusal: Some(HostRefusal {
                function: "f".to_owned(),
                errc: "e".to_owned(),
                message: "x".to_owned(),
                subject: Some(Place { file: 0, lo: 1, hi: 2 }),
            }),
        }),
        files: vec![file(b"/s/a.cc", 10, 1)],
        ..empty()
    });
    for x in bases {
        let base = map_shard(x).to_bytes();
        for v in variants(*b"SHRD", &base, 17) {
            shard_survives(&v);
        }
    }
}

// spec/edits.md §3, spec/model.md §2.1: likewise for a rename request.
#[test]
fn corrupted_transform_requests() {
    let t = Transform {
        kind: TransformKind::Rename,
        ownership: Ownership { owned: vec![b"/s".to_vec()], readonly: vec![b"/s/gen".to_vec()] },
        indivisible: true,
        rename: Some(Rename {
            target: vec![b"n".to_vec(), b"f".to_vec()],
            kind: TargetKind::Function,
            new_name: b"g".to_vec(),
        }),
        script: None,
    };
    for v in variants(*b"TRFM", &t.to_bytes(), 1) {
        let r = catch_unwind(AssertUnwindSafe(|| Transform::from_bytes(&v)));
        let Ok(r) = r else { panic!("from_bytes panicked") };
        if let Ok(t) = r {
            t.validate().unwrap();
            assert_eq!(t.to_bytes(), v);
            assert_eq!(Transform::from_json(&t.to_json()).unwrap(), t);
        }
    }
}
