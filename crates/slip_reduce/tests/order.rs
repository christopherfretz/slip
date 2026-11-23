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
//!   (landed WP-062 §8: it is the first observer's too);
//! - every set validates (§6.1) and survives its binary form;
//! - the naive model of `common/model.rs` makes the same set.
//!
//! The seed is printed on failure; `SLIP_REDUCE_SEED` gives another.

#![allow(clippy::many_single_char_names, clippy::too_many_lines, clippy::manual_assert, clippy::explicit_auto_deref)]

mod common;

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::*;
use slip_model::editset::EditSet;
use slip_model::edits::{Reason, Severity, Stance};
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
    if r.chance(20) {
        c = c.notes(&[*pick(r, &["n1", "n2", "n3"])]);
    }
    c.votes = 1 + r.below(2);
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
            let (m, subject) = if r.chance(50) { ("boom", Some((FILES[0], 1, 2))) } else { ("bang", None) };
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
                subjects.extend(c.subjects.iter().copied());
                b = b.claim(c.clone());
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
        let replaces: Vec<usize> =
            b.claims.iter().enumerate().filter(|(_, c)| c.stance == Stance::Replace).map(|(k, _)| k).collect();
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
            let named = format!("{:?}{:?}{:?}", b.claims, b.reports, b.changed).contains(&format!("{:?}", FILES[1].as_bytes()));
            b = if named { b.file(FILES[1], SIZE + 5, d(0xb2)) } else { b.input(FILES[1], SIZE + 5, d(0xb2)) };
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
            Some(s) => {
                let id = id_of(s);
                Entry::Shard(Box::new(s.clone()), Some(id))
            }
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
    assert_eq!(slip_reduce::declared(&base.edits).unwrap(), base.declared, "§6.2: one derivation");

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

/// §1 rule 3, §5.3, §5.6, §6.1 and the model, over generated sets.
#[test]
fn generated_sets_are_order_independent_valid_and_agree_with_the_model() {
    let base: u64 = std::env::var("SLIP_REDUCE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x062_5eed);
    let mut tally = Tally { causes: std::collections::BTreeMap::new(), cases: 0, reduces: 0, accepted: 0, edits: 0 };
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
    assert!(tally.accepted >= CASES / 10, "the generator makes too few accepted sets: {}", tally.accepted);
}
