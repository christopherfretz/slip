//! Memory and time (`spec/edits.md` §5.6, `docs/wp/WP-062.md` "Tests"):
//! one site claimed by ten thousand shards with `fan_in` 2 stays within a
//! stated bound, and that bound does not grow with the number of shards;
//! a generated run of ten thousand shards, timed and measured.
//!
//! The workspace forbids `unsafe` code, tests included, so a counting
//! global allocator cannot be written (landed WP-062 §9). The peak is
//! measured instead as the growth of the process's resident high-water
//! mark (`VmHWM` of `/proc/self/status`) across one call, after resetting
//! it (`/proc/self/clear_refs`, Linux 4.0 and later). That counts every
//! page the call touched, the allocator's slack included: an upper bound
//! on live heap. The tests of this binary run one at a time (a lock), so
//! that no other test's pages are counted. Where the counters are not
//! there, the bound is not checked and the test says so.

mod common;

use std::sync::Mutex;
use std::time::Instant;

use common::*;
use slip_model::editset::SetState;
use slip_model::formats::content_id;
use slip_model::{Digest, Shard};
use slip_reduce::{Options, Shards, reduce};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// `VmHWM` and `VmRSS` in KiB.
fn status() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    Some((field("VmHWM:")?, field("VmRSS:")?))
}

/// The growth of the high-water mark over `f`, in KiB, if it can be
/// measured.
fn peak_growth<T>(f: impl FnOnce() -> T) -> (T, Option<u64>) {
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    let before = status();
    let out = f();
    let after = status();
    let growth = match (reset, before, after) {
        (true, Some((_, rss)), Some((hwm, _))) => Some(hwm.saturating_sub(rss)),
        _ => None,
    };
    (out, growth)
}

/// Shards made when opened, so that the store holds nothing: shard `i` of
/// `make(i)`.
struct Lazy {
    n: usize,
    make: fn(usize) -> Shard,
}

impl Shards for Lazy {
    fn open(&self, index: usize) -> Result<Option<Shard>, String> {
        Ok(Some((self.make)(index)))
    }
    fn len(&self) -> usize {
        self.n
    }
    fn observer(&self, index: usize) -> Digest {
        observer(index)
    }
    fn shard_id(&self, index: usize) -> Option<Digest> {
        content_id(&(self.make)(index).to_bytes()).ok()
    }
}

/// Every shard replaces one byte the same way.
fn one_site(i: usize) -> Shard {
    ShardB::new(i).claim(replace("/s/a.cc", 10, 11, vec![lit("g")]).notes(&["the rename"])).build()
}

/// §5.6: one site claimed by every observer costs one record at every
/// level. With `fan_in` 2 and one thread, reduce holds two shards' `edits`
/// sections, one record per open temporary file and its buffer (two read,
/// one written: 8 KiB each), the joined state (nothing here) and the edit
/// set (one edit, eight `first` digests): well under 100 KiB. The bound
/// below, 1 MiB of growth, is that plus the allocator's slack (thread
/// stacks are not used with one thread); and the growth from a thousand
/// shards to ten thousand stays within 256 KiB, since nothing held grows
/// with the observers or the runs. (A state kept per run, 5,000 runs of
/// 2 shards, grew it by about 1 MiB.)
#[test]
fn one_site_ten_thousand_shards_fan_in_two() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut growth = Vec::new();
    // The first call of the process warms the allocator and the standard
    // library; it is not compared.
    for n in [1_000, 1_000, 10_000] {
        let store = Lazy { n, make: one_site };
        let o = Options { fan_in: 2, threads: 1, scratch: scratch("one-site") };
        let start = Instant::now();
        let (r, g) = peak_growth(|| reduce(&script(), REQUEST, PLAN, 0, &store, &o).unwrap());
        let took = start.elapsed();
        assert_clean(&o.scratch);
        let set = r.edits;
        assert_eq!(set.state, SetState::Accepted);
        assert_eq!(set.edits.len(), 1);
        assert_eq!(set.edits[0].observers, n as u64);
        assert_eq!(set.edits[0].votes, n as u64);
        assert_eq!(set.edits[0].first.len(), 8);
        assert_eq!(counter(&set, "reduce.claims"), n as u64);
        eprintln!("one site, {n} shards, fan_in 2: {took:?}, peak growth {g:?} KiB");
        growth.push(g);
    }
    match (growth[1], growth[2]) {
        (Some(small), Some(large)) => {
            assert!(large <= 1024, "one site of 10000 shards grew the peak by {large} KiB");
            assert!(
                large <= small + 256,
                "the peak grows with the shards: {small} KiB for 1000, {large} KiB for 10000"
            );
        }
        _ => eprintln!("the resident high-water mark cannot be measured here: the bound is not checked"),
    }
}

/// A rename-like run: every shard claims the same 8 sites of a shared
/// header and 12 sites of a file of its own, with a group-free claim set
/// (a native rename's). Not one site in common between two own files.
fn rename_like(i: usize) -> Shard {
    let mut b = ShardB::new(i);
    for k in 0..8 {
        b = b.claim(replace("/s/include/shared.h", 100 * k, 100 * k + 3, vec![lit("new")]));
    }
    let own = format!("/s/src/f{i}.cc");
    for k in 0..12 {
        b = b.claim(replace(&own, 50 * k, 50 * k + 3, vec![lit("new")]));
    }
    b.build()
}

fn large_run(n: usize, threads: usize) {
    let store = Lazy { n, make: rename_like };
    let o = Options { fan_in: 64, threads, scratch: scratch("large") };
    let start = Instant::now();
    let (r, g) = peak_growth(|| reduce(&script(), REQUEST, PLAN, 0, &store, &o).unwrap());
    let took = start.elapsed();
    assert_clean(&o.scratch);
    let set = r.edits;
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.edits.len(), 8 + 12 * n);
    assert_eq!(counter(&set, "reduce.edits.shared"), 8);
    eprintln!("rename-like run, {n} shards, {} claims, {threads} threads: {took:?}, peak growth {g:?} KiB", 20 * n);
}

/// §5.6: a generated run of two thousand shards, in every `cargo test`.
#[test]
fn rename_like_two_thousand_shards() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    large_run(2_000, 4);
}

/// §5.6: the large run of the handback, ten thousand shards and 200,000
/// claims: `cargo test -p slip_reduce --test memory -- --ignored
/// --nocapture`.
#[test]
#[ignore = "the large run of the handback; minutes in a debug build"]
fn rename_like_ten_thousand_shards() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    large_run(10_000, 8);
}
