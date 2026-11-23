//! Order-independence and the model (`spec/edits.md` §1 rule 3, §5.3,
//! §5.6, `docs/wp/WP-062.md` "Tests"): for claim sets generated from a
//! fixed seed, with copies, nested edits, groups, reports, `changed`
//! entries, failed runs, unparsed observers, rules and file disagreements
//! among them,
//!
//! - the `EDTS` and `DECL` bytes are identical under every `fan_in` in
//!   {2, 3, 7, 64} and `threads` in {1, 2, 8}, with a `Shards` that delays
//!   each `open` pseudo-randomly so that runs finish in different orders;
//! - under a permutation of the plan's order the set differs only in
//!   `basis`, `unparsed`, `first`, `display`, and a failure's `rendered`
//!   (landed WP-062 §4: it is the first observer's too);
//! - every set validates (§6.1) and survives its binary form;
//! - the naive model of `common/model.rs` makes the same set.
//!
//! The seed is printed on failure; `SLIP_REDUCE_SEED` gives another.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::manual_assert,
    clippy::explicit_auto_deref
)]

mod common;

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::*;
use slip_model::edits::{Reason, Severity, Stance};
use slip_model::editset::EditSet;
use slip_model::transform::Transform;
use slip_model::{Digest, Shard, Status};
use slip_reduce::{Options, Reduced, reduce};

const FILES: [&str; 3] = ["/s/a.cc", "/s/b.h", "/s/c.h"];
const READ_ONLY: &str = "/s/ro/r.h";
const CASES: u64 = 160;

fn pick<'a, T>(r: &mut Rng, xs: &'a [T]) -> &'a T {
    &xs[r.below(xs.len() as u64) as usize]
}

fn range(r: &mut Rng, path: &str, min_len: u64) -> (String, u64, u64) {
    let lo = r.below(60);
    let len = min_len + r.below(8);
    (path.to_owned(), lo, lo + len)
}

/// One claim of the case's pool: what several observers may claim alike.
fn pool_claim(r: &mut Rng) -> C {
    let stance = match r.below(100) {
        0..55 => Stance::Replace,
        55..72 => Stance::Keep,
        72..84 => Stance::Refuse,
        _ => Stance::Unresolved,
    };
    let path = if stance != Stance::Replace && r.chance(10) { READ_ONLY } else { *pick(r, &FILES) };
    let empty_ok = stance != Stance::Replace && r.chance(5);
    let (p, lo, hi) = range(r, path, u64::from(!empty_ok));
    let mut c = C::new(&p, lo, hi, stance);
    if stance == Stance::Replace {
        let lit_ = |r: &mut Rng| lit(*pick(r, &["a", "b", "c"]));
        let copy_ = |r: &mut Rng| {
            let f = *pick(r, &FILES);
            let (cp, clo, chi) = range(r, f, 1);
            copy(&cp, clo, chi + r.below(6))
        };
        c.pieces = match r.below(7) {
            0 | 1 => vec![lit_(r)],
            2 => vec![spelled(d(1 + r.below(2) as u8), "n")],
            3 => vec![copy_(r)],
            4 => vec![lit_(r), copy_(r)],
            5 => vec![copy_(r), lit_(r), copy_(r)],
            _ => vec![lit_(r), copy_(r), lit_(r)],
        };
    }
    if stance == Stance::Refuse {
        c.reason = *pick(r, &[Reason::Crossing, Reason::MacroBody, Reason::Pasted]);
    }
    if r.chance(50) {
        let n = 1 + r.below(2);
        let s: Vec<Digest> = (0..n).map(|_| d(1 + r.below(6) as u8)).collect();
        c = c.subjects(&s);
    }
    if r.chance(15) {
        c = c.in_macro(lo.saturating_sub(1), *pick(r, &["M", "ID"]));
    }
    if r.chance(30) {
        c = c.notes(&some_notes(r));
    }
    c.votes = 1 + r.below(2);
    c
}

/// One to three of twelve notes: a claim joined over observers has more
/// than eight (§5.3: the least eight are kept, whatever the order).
fn some_notes(r: &mut Rng) -> Vec<&'static str> {
    const NOTES: [&str; 12] =
        ["n00", "n01", "n02", "n03", "n04", "n05", "n06", "n07", "n08", "n09", "n10", "n11"];
    (0..=r.below(3)).map(|_| *pick(r, &NOTES)).collect()
}

/// What one observer makes of a pool claim: the same site, stance and
/// pieces (the same collapsed claim), with a part of its subjects, notes
/// and a macro name of its own at times. The subjects then join by union (§5.3, and
/// `mixed_set` may follow), the notes keep the least eight, the macro the
/// least (§4.3: a tie on `name_start` broken by the name).
fn as_seen(r: &mut Rng, c: &C) -> C {
    let mut c = c.clone();
    if r.chance(25) {
        // A part of the claim's subjects: two observers may each see one
        // of two (mutation M38).
        let s: Vec<Digest> = c.subjects.iter().copied().filter(|_| r.chance(50)).collect();
        c = c.subjects(&s);
    }
    if r.chance(35) {
        c = c.notes(&some_notes(r));
    }
    if let Some(m) = c.macro_site.clone()
        && r.chance(40)
    {
        c = c.in_macro(m.name_start, *pick(r, &["M", "ID", "A", "Z"]));
    }
    c
}

fn rule_of(id: u8) -> Option<slip_model::keys::Rule> {
    match id {
        1 => Some(function_rule("f", "g")),
        2 => Some(function_rule("h", "i")),
        _ => None,
    }
}

/// A generated case: the request and the shards in plan order (`None`:
/// unparsed).
fn generate(seed: u64) -> (Transform, Vec<(Digest, Option<Shard>)>) {
    let mut r = Rng(seed);
    let request = if r.chance(25) { rename() } else { script() };
    let n = 1 + r.below(12) as usize;
    let mut pool: Vec<C> = (0..4 + r.below(13)).map(|_| pool_claim(&mut r)).collect();
    // Another replacement of a range some claim replaces: `divergent`.
    if let Some(c) = pool.iter().find(|c| c.stance == Stance::Replace).cloned()
        && r.chance(30)
    {
        pool.push(C { pieces: vec![lit("z")], ..c });
    }
    // Disagreement around other edits (landed WP-062 §5): a replacement of
    // a range some claim replaces, and a replacement inside one, so that a
    // site some observers do not see is `divergent` around or across the
    // others.
    // Half the cases, so that enough sets are accepted.
    let replaces: Vec<C> = pool.iter().filter(|c| c.stance == Stance::Replace).cloned().collect();
    let disagree = r.chance(50);
    for c in replaces.into_iter().filter(|_| disagree) {
        if r.chance(12) {
            pool.push(C { pieces: vec![lit("w")], ..c.clone() });
        }
        if c.hi - c.lo >= 3 && r.chance(20) {
            let lo = c.lo + 1 + r.below(c.hi - c.lo - 2);
            pool.push(C { lo, hi: lo + 1, pieces: vec![lit("i")], ..c });
        }
    }
    let mut out = Vec::new();
    for i in 0..n {
        let roll = r.below(100);
        if roll < 7 {
            out.push((observer(i), None));
            continue;
        }
        let mut b = ShardB::new(i);
        if roll < 12 {
            out.push((observer(i), Some(b.status(Status::Diagnosed).build())));
            continue;
        }
        if roll < 20 {
            let (m, subject) =
                if r.chance(50) { ("boom", Some((FILES[0], 1, 2))) } else { ("bang", None) };
            out.push((observer(i), Some(b.failed(m, subject).build())));
            continue;
        }
        let mut keys = BTreeSet::new();
        let mut subjects = BTreeSet::new();
        for c in &pool {
            if !r.chance(50) {
                continue;
            }
            let key = format!("{:?}{}{}{:?}{:?}", c.path, c.lo, c.hi, c.stance, c.pieces);
            if keys.insert(key) {
                let c = as_seen(&mut r, c);
                subjects.extend(c.subjects.iter().copied());
                b = b.claim(c);
            }
        }
        for id in 1..=6u8 {
            if r.chance(30) {
                subjects.insert(d(id));
            }
        }
        for &k in &subjects {
            let id = k.0[0];
            b = match id {
                5 => b.follower(k, &[d(1)]),
                6 => b.follower(k, &[d(2), d(3)]),
                _ if r.chance(30) => b.seed(k),
                _ => b.candidate(k),
            };
            if let Some(rule) = rule_of(id) {
                b = b.rule(k, rule);
            }
            if r.chance(4) {
                b = b.flags(k, 1 << r.below(4));
            }
        }
        let ids: Vec<Digest> = subjects.iter().copied().collect();
        for _ in 0..r.below(3) {
            if ids.len() >= 2 {
                let (a, c) = (*pick(&mut r, &ids), *pick(&mut r, &ids));
                if a != c {
                    b = b.link(a, c);
                }
            }
        }
        // Groups: disjoint sets of two or more replace claims.
        let replaces: Vec<usize> = b
            .claims
            .iter()
            .enumerate()
            .filter(|(_, c)| c.stance == Stance::Replace)
            .map(|(k, _)| k)
            .collect();
        if replaces.len() >= 2 && r.chance(40) {
            let mut rest = replaces.clone();
            while rest.len() >= 2 && r.chance(70) {
                let take = 2 + r.below((rest.len() - 1) as u64) as usize;
                let g: Vec<usize> = rest.drain(..take.min(rest.len())).collect();
                b = b.group(&g);
            }
        }
        if r.chance(30) {
            b = b.report(Some((FILES[0], 0, 1)), Severity::Review, "look");
        }
        if r.chance(20) {
            b = b.report(None, Severity::Note, "n");
        }
        if r.chance(10) {
            b = b.report(Some((FILES[1], 5, 9)), Severity::Error, "bad");
        }
        if r.chance(20) {
            b = b.changed(d(0x20), FILES[0], 0, 10, "callee");
        }
        if r.chance(15) {
            b = b.changed(d(0x21), FILES[1], 20, 30, "other");
        }
        if r.chance(3) {
            b = b.renamed(function_rule("f", "h"));
        } else if r.chance(20) {
            b = b.renamed(function_rule("f", "g"));
        }
        if r.chance(10) {
            b = b.renamed(namespace_rule("n", "m"));
        }
        if r.chance(4) {
            b = b.file(FILES[0], SIZE, d(0xa2));
        }
        if r.chance(3) {
            // In `inputs` alone if the section names no place in it, else
            // in `files` (§5.1: the two agree within a shard).
            let named = format!("{:?}{:?}{:?}", b.claims, b.reports, b.changed)
                .contains(&format!("{:?}", FILES[1].as_bytes()));
            b = if named {
                b.file(FILES[1], SIZE + 5, d(0xb2))
            } else {
                b.input(FILES[1], SIZE + 5, d(0xb2))
            };
        }
        let n_claims = b.claims.len() as u64;
        b = b.counter("edits.claims", n_claims).counter("api.peak_bytes", r.below(1000));
        let s = b.build();
        s.validate().unwrap_or_else(|e| panic!("the generator made an invalid shard: {e}"));
        out.push((observer(i), Some(s)));
    }
    (request, out)
}

fn store_of(shards: &[(Digest, Option<Shard>)]) -> Store {
    let mut store = Store::new(Vec::new());
    for (o, s) in shards {
        store.observers.push(*o);
        store.entries.push(match s {
            None => Entry::Missing,
            Some(s) => Entry::Bytes(s.to_bytes()),
        });
    }
    store
}

fn reduce_with(request: &Transform, store: &Store, fan_in: usize, threads: usize) -> Reduced {
    let o = Options { fan_in, threads, scratch: scratch("order") };
    let r = reduce(request, REQUEST, PLAN, 0, store, &o)
        .unwrap_or_else(|e| panic!("generated shards raise no Input error: {e}"));
    assert_clean(&o.scratch);
    r
}

fn bytes(r: &Reduced) -> (Vec<u8>, Option<Vec<u8>>) {
    (r.edits.to_bytes(), r.declared.as_ref().map(slip_model::declared::Declared::to_bytes))
}

/// What a permutation of the plan may change, cleared.
fn normal(mut s: EditSet) -> EditSet {
    s.basis = Digest([0; 32]);
    s.unparsed.sort();
    for e in &mut s.edits {
        e.first.clear();
    }
    for c in &mut s.changed {
        c.first.clear();
    }
    for r in &mut s.refusals {
        r.first.clear();
    }
    for r in &mut s.reports {
        r.first.clear();
    }
    for f in &mut s.failures {
        f.first.clear();
        f.failure.rendered.clear();
    }
    for x in &mut s.subjects {
        x.display.clear();
    }
    s
}

fn permuted(r: &mut Rng, shards: &[(Digest, Option<Shard>)]) -> Vec<(Digest, Option<Shard>)> {
    let mut v = shards.to_vec();
    for i in (1..v.len()).rev() {
        let j = r.below(i as u64 + 1) as usize;
        v.swap(i, j);
    }
    v
}

struct Tally {
    causes: std::collections::BTreeMap<String, u64>,
    cases: u64,
    reduces: u64,
    accepted: u64,
    edits: u64,
}

fn check(seed: u64, tally: &mut Tally) {
    let (request, shards) = generate(seed);
    let store = store_of(&shards);
    let base = reduce_with(&request, &store, 64, 1);
    tally.cases += 1;
    for r in &base.edits.refusals {
        *tally.causes.entry(format!("{:?}", r.cause)).or_default() += 1;
    }
    tally.reduces += 1;
    tally.edits += base.edits.edits.len() as u64;
    tally.accepted += u64::from(base.edits.state == slip_model::editset::SetState::Accepted);

    // §6.1: valid, and the same after its binary form.
    base.edits.validate().unwrap_or_else(|e| panic!("reduce made an invalid set: {e}"));
    assert_eq!(EditSet::from_bytes(&base.edits.to_bytes()).unwrap(), base.edits);
    assert_eq!(
        slip_reduce::declared(&base.edits, &request.ownership).unwrap(),
        base.declared,
        "§6.2: one derivation"
    );

    // Landed WP-062 §6: a missing edit is explained.
    explained(&request, &shards, &base.edits);

    // The model.
    let with_ids: Vec<(Digest, Option<Shard>, Option<Digest>)> =
        shards.iter().map(|(o, s)| (*o, s.clone(), s.as_ref().map(id_of))).collect();
    let model = model::reduce(&request, REQUEST, PLAN, 0, &with_ids);
    if model != base.edits {
        panic!("reduce and the model disagree\nreduce: {:#?}\nmodel: {:#?}", base.edits, model);
    }

    // §5.3, §5.6: fan_in, threads and the order in which runs finish.
    let want = bytes(&base);
    let mut delayed = store_of(&shards);
    delayed.delay = Some(seed);
    for fan_in in [2, 3, 7, 64] {
        for threads in [1, 2, 8] {
            let s = if threads > 1 { &delayed } else { &store };
            let got = bytes(&reduce_with(&request, s, fan_in, threads));
            tally.reduces += 1;
            assert!(got == want, "fan_in {fan_in}, threads {threads}: the bytes differ");
        }
    }

    // §1 rule 3: the plan's order.
    let mut r = Rng(seed ^ 0x5eed);
    for _ in 0..2 {
        let p = permuted(&mut r, &shards);
        let got = reduce_with(&request, &store_of(&p), 3, 2);
        tally.reduces += 1;
        let (a, b) = (normal(got.edits), normal(base.edits.clone()));
        if a != b {
            panic!("a permutation of the plan changes the set\npermuted: {a:#?}\nbase: {b:#?}");
        }
    }
}

type SiteP = (Vec<u8>, u64, u64);

/// Landed WP-062 §6: every site at which an input shard claims a `replace`
/// is an edit of the set, the site of a refusal (`with_group` for one
/// taken down with its group), or `kept` (§5.2 made the claim a `keep`);
/// or the set is refused as a whole: under an `indivisible` request, or
/// with a refusal of no site.
fn explained(request: &Transform, shards: &[(Digest, Option<Shard>)], set: &EditSet) {
    let path = |f: u64| set.files[f as usize].path.clone();
    let whole = set.state == slip_model::editset::SetState::Refused
        && (request.indivisible || set.refusals.iter().any(|r| r.file.is_none()));
    if whole {
        return;
    }
    let mut known: BTreeSet<SiteP> = BTreeSet::new();
    known.extend(set.edits.iter().map(|e| (path(e.file), e.lo, e.hi)));
    known.extend(set.refusals.iter().filter_map(|r| r.file.map(|f| (path(f), r.lo, r.hi))));
    known.extend(set.kept.iter().map(|k| (path(k.file), k.lo, k.hi)));
    for (_, s) in shards {
        let Some(s) = s else { continue };
        let Some(edits) = s.edits.as_ref().filter(|e| e.failure.is_none()) else { continue };
        if s.outcome.status != Status::Ok {
            continue;
        }
        for c in edits.claims.iter().filter(|c| c.stance == Stance::Replace) {
            let site = (edits.files[c.file as usize].path.clone(), c.lo, c.hi);
            assert!(known.contains(&site), "the replace site {site:?} is not explained: {set:#?}");
        }
    }
}

/// The site refusals of a set, by site, with their causes.
fn refused_sites(set: &EditSet) -> std::collections::BTreeMap<SiteP, Vec<String>> {
    let mut out: std::collections::BTreeMap<SiteP, Vec<String>> = std::collections::BTreeMap::new();
    for r in &set.refusals {
        if let Some(f) = r.file {
            let site = (set.files[f as usize].path.clone(), r.lo, r.hi);
            out.entry(site).or_default().push(format!("{:?}", r.cause));
        }
    }
    out
}

/// What §5.2 made of a set: the selected subjects, the refusals of no
/// site, the kept sites and the `mixed_set` sites.
fn selection_of(set: &EditSet) -> (Vec<Digest>, Vec<String>, Vec<SiteP>) {
    let path = |f: u64| set.files[f as usize].path.clone();
    let mut sites: Vec<SiteP> = set.kept.iter().map(|k| (path(k.file), k.lo, k.hi)).collect();
    sites.extend(
        set.refusals
            .iter()
            .filter(|r| r.cause == slip_model::editset::Cause::MixedSet)
            .filter_map(|r| r.file.map(|f| (path(f), r.lo, r.hi))),
    );
    let alone = set
        .refusals
        .iter()
        .filter(|r| r.file.is_none())
        .map(|r| format!("{:?} {:?}", r.cause, r.subject))
        .collect();
    (set.subjects.iter().map(|x| x.id).collect(), alone, sites)
}

/// An edit `reduce(S)` refuses that `reduce(S ∪ {s})` accepts: `s` the
/// shard at `k` of the case, the site, its causes in `reduce(S)`, whether
/// §5.2 made something else of the two (`selection_of`), and whether `s`
/// carries it: a `replace` of `s` copies a range the site is properly
/// inside, and an edit of `reduce(S ∪ {s})` does.
type Exception = (usize, SiteP, Vec<String>, bool, bool);

fn inside(x: &SiteP, r: &SiteP) -> bool {
    x.0 == r.0 && r.1 <= x.1 && x.2 <= r.2 && (x.1, x.2) != (r.1, r.2)
}

/// Whether shard `s` and the set `whole` both copy a range `x` is
/// properly inside.
fn carries(s: &Shard, whole: &EditSet, x: &SiteP) -> bool {
    let by_shard = s.edits.iter().any(|e| {
        e.claims.iter().filter(|c| c.stance == Stance::Replace).any(|c| {
            c.pieces.iter().any(|p| match p {
                slip_model::edits::Piece::Copy(r) => {
                    inside(x, &(e.files[r.file as usize].path.clone(), r.lo, r.hi))
                }
                _ => false,
            })
        })
    });
    let by_set = whole.edits.iter().any(|e| {
        e.pieces.iter().any(|p| match p {
            slip_model::edits::Piece::Copy(r) => {
                inside(x, &(whole.files[r.file as usize].path.clone(), r.lo, r.hi))
            }
            _ => false,
        })
    });
    by_shard && by_set
}

/// Monotone refusal (landed WP-062 §5): for the shards `S` of a case less
/// one, `s`, every edit `reduce(S)` refuses is refused or absent in
/// `reduce(S ∪ {s})`. The cases where it does not hold.
fn monotone(
    request: &Transform,
    shards: &[(Digest, Option<Shard>)],
    whole: &EditSet,
) -> Vec<Exception> {
    let path = |f: u64| whole.files[f as usize].path.clone();
    let accepted: BTreeSet<SiteP> =
        whole.edits.iter().map(|e| (path(e.file), e.lo, e.hi)).collect();
    let mut out = Vec::new();
    for k in 0..shards.len() {
        if shards[k].1.is_none() {
            continue;
        }
        let mut less = shards.to_vec();
        less.remove(k);
        let part = reduce_with(request, &store_of(&less), 64, 1);
        let selection = selection_of(&part.edits) != selection_of(whole);
        for (site, causes) in refused_sites(&part.edits) {
            if accepted.contains(&site) {
                let carried = shards[k].1.as_ref().is_some_and(|s| carries(s, whole, &site));
                out.push((k, site, causes, selection, carried));
            }
        }
    }
    out
}

/// The case with no claim naming a subject: §5.2 then touches no claim
/// (a claim with no subject is not touched by selection), so that what
/// one more shard changes is disagreement alone.
fn without_subjects(shards: &[(Digest, Option<Shard>)]) -> Vec<(Digest, Option<Shard>)> {
    let mut v = shards.to_vec();
    for (_, s) in &mut v {
        if let Some(e) = s.as_mut().and_then(|s| s.edits.as_mut()) {
            for c in &mut e.claims {
                c.subjects.clear();
            }
        }
        if let Some(s) = s {
            s.validate().unwrap_or_else(|e| panic!("a claim without subjects: {e}"));
        }
    }
    v
}

/// Monotone refusal over generated sets: more disagreement never accepts
/// more (landed WP-062 §5).
///
/// - With no claim naming a subject, an exception is only of the kind
///   landed §5 names: an edit `buried` in `S` that `s`'s copy carries.
/// - As generated, an exception is that, or one where `s` changes what
///   §5.2 makes of the claims: it names a seed, a link or a subject that
///   selects or deselects, so that a `replace` becomes a `keep` (whose
///   pieces are gone) or a `mixed_set` an edit. Selection is of all the
///   shards; it is not disagreement.
///
/// Every exception is printed.
#[test]
fn more_shards_refuse_no_less() {
    let base: u64 =
        std::env::var("SLIP_REDUCE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x062_5eed);
    let show = |what: &str, seed: u64, (k, site, causes, selection, carried): &Exception| {
        eprintln!(
            "monotone {what}: seed {seed}: without shard {k}, {} [{}, {}) is {causes:?}; \
             accepted with it{}",
            String::from_utf8_lossy(&site.0),
            site.1,
            site.2,
            if *selection { " (§5.2 differs)" } else { "" }
        );
        if *carried {
            eprintln!("  carried by the shard's copy");
        }
    };
    let (mut effect, mut selection, mut reductions) = (0, 0, 0);
    for case in 0..CASES {
        let seed = base.wrapping_add(case.wrapping_mul(0x9e37_79b9));
        let (request, shards) = generate(seed);
        for (what, shards) in [("plain", without_subjects(&shards)), ("as generated", shards)] {
            let whole = reduce_with(&request, &store_of(&shards), 64, 1);
            reductions += shards.len();
            for x in monotone(&request, &shards, &whole.edits) {
                show(what, seed, &x);
                // The effect: buried in S, or taken down with a group by a
                // buried member, and carried by s's copy in S ∪ {s}.
                let buried = x.2.iter().any(|c| c == "Buried" || c == "WithGroup");
                if buried && x.4 {
                    effect += 1;
                } else if x.3 && what == "as generated" {
                    selection += 1;
                } else {
                    panic!("seed {seed}: more disagreement accepts more: {x:?}");
                }
            }
        }
    }
    eprintln!(
        "monotone: {CASES} cases, {reductions} reductions less one; exceptions: {effect} of the \
         effect, {selection} of §5.2"
    );
}

/// §1 rule 3, §5.3, §5.6, §6.1 and the model, over generated sets.
#[test]
fn generated_sets_are_order_independent_valid_and_agree_with_the_model() {
    let base: u64 =
        std::env::var("SLIP_REDUCE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x062_5eed);
    let mut tally = Tally {
        causes: std::collections::BTreeMap::new(),
        cases: 0,
        reduces: 0,
        accepted: 0,
        edits: 0,
    };
    for case in 0..CASES {
        let seed = base.wrapping_add(case.wrapping_mul(0x9e37_79b9));
        let r = catch_unwind(AssertUnwindSafe(|| check(seed, &mut tally)));
        if r.is_err() {
            panic!("failed at case {case} of SLIP_REDUCE_SEED={base} (case seed {seed})");
        }
    }
    eprintln!(
        "order: {} cases, {} reductions, {} accepted sets, {} edits; refusals {:?}",
        tally.cases, tally.reduces, tally.accepted, tally.edits, tally.causes
    );
    assert!(
        tally.accepted >= CASES / 10,
        "the generator makes too few accepted sets: {}",
        tally.accepted
    );
}
