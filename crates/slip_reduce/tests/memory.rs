//! Memory and time (`spec/edits.md` §5.6, `docs/wp/WP-062.md` "Tests",
//! landed WP-142): what reduce holds, counted exactly by the call itself
//! (`Reduced::peak`, `reduce.peak.*`), for one site claimed by a thousand
//! and by ten thousand shards with `fan_in` 2, and the one term that grows
//! with the observers; a generated run of thousands of shards, timed.
//!
//! The resident high-water mark of the process is read only by the
//! `#[ignore]`d tests, against an absolute bound: on Linux 6.2 and later
//! the resident counters are per-CPU and read approximately, so a reading
//! cannot resolve a difference of a few hundred KiB (landed WP-142 §3),
//! and the gate does not depend on one.
//!
//! The workspace forbids `unsafe` code, tests included, so a counting
//! global allocator cannot be written (landed WP-062 §9). The reading is
//! the growth of the process's resident high-water mark (`VmHWM` of
//! `/proc/self/status`) across one call, after resetting it
//! (`/proc/self/clear_refs`, Linux 4.0 and later). That counts every page
//! the call touched, the allocator's slack included: an upper bound on
//! live heap. The tests of this binary run one at a time (a lock), so that
//! no other test's pages are counted. Where the counters are not there,
//! the bound is not checked and the test says so.
//!
//! The high-water mark counts file-backed pages too (the binary's own
//! text, mapped libraries). On a host short of memory the kernel drops
//! such pages and the call faults them in again, which raises the mark
//! by hundreds of KiB with no allocation at all (seen 2026-11-23 beside
//! full builds: 268 to 608 KiB for a call that grows by 0 on a quiet
//! host). So the growth of the file-backed and shared resident pages
//! over the call (`RssFile` and `RssShmem`, Linux 4.5 and later) is
//! taken off: what is left bounds the anonymous memory, which is what
//! the section's claim is about. Where those two fields are missing
//! nothing is taken off.

mod common;

use std::sync::Mutex;
use std::time::Instant;

use common::*;
use slip_model::editset::SetState;
use slip_model::{Digest, Shard};
use slip_reduce::{Options, Peak, Reduced, Shards, WINDOW_PER_THREAD, reduce};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// What `/proc/self/status` says, in KiB.
#[derive(Clone, Copy, Debug)]
struct Status {
    /// `VmHWM`: the resident high-water mark.
    hwm: u64,
    /// `VmRSS`.
    rss: u64,
    /// `RssFile` and `RssShmem`: the resident pages that are not
    /// anonymous (0 where the kernel does not say).
    file: u64,
}

fn status() -> Option<Status> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    Some(Status {
        hwm: field("VmHWM:")?,
        rss: field("VmRSS:")?,
        file: field("RssFile:").unwrap_or(0) + field("RssShmem:").unwrap_or(0),
    })
}

/// The growth of the high-water mark over `f`, less the growth of the
/// resident pages that are not anonymous (module documentation), in KiB,
/// if it can be measured. The two readings are printed, so that a bound
/// that fails says which kind of page grew.
fn peak_growth<T>(f: impl FnOnce() -> T) -> (T, Option<u64>) {
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    let before = status();
    let out = f();
    let after = status();
    let growth = match (reset, before, after) {
        (true, Some(b), Some(a)) => {
            eprintln!("resident before {b:?}, after {a:?}");
            let not_anonymous = a.file.saturating_sub(b.file);
            Some(a.hwm.saturating_sub(b.rss).saturating_sub(not_anonymous))
        }
        _ => None,
    };
    (out, growth)
}

/// Shards made when opened, so that the store holds nothing: shard `i` of
/// `make(i)`, or none where `unparsed(i)`.
struct Lazy {
    n: usize,
    make: fn(usize) -> Shard,
    unparsed: fn(usize) -> bool,
}

impl Shards for Lazy {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        Ok((!(self.unparsed)(index)).then(|| (self.make)(index).to_bytes()))
    }
    fn len(&self) -> usize {
        self.n
    }
    fn observer(&self, index: usize) -> Digest {
        observer(index)
    }
}

/// Every shard replaces one byte the same way.
fn one_site(i: usize) -> Shard {
    ShardB::new(i).claim(replace("/s/a.cc", 10, 11, vec![lit("g")]).notes(&["the rename"])).build()
}

/// No observer is unparsed.
fn none(_: usize) -> bool {
    false
}

/// One call over `n` shards of [`one_site`], none where `unparsed`,
/// `fan_in` 2, one thread; the edit set is checked as one site claimed by
/// every `ok` shard.
fn one_site_run(n: usize, unparsed: fn(usize) -> bool, ok: u64) -> Reduced {
    let store = Lazy { n, make: one_site, unparsed };
    let o = Options { fan_in: 2, threads: 1, scratch: scratch("one-site") };
    let start = Instant::now();
    let r = reduce(&script(), REQUEST, PLAN, 0, &store, &o).unwrap();
    let took = start.elapsed();
    assert_clean(&o.scratch);
    let set = &r.edits;
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.edits.len(), 1);
    assert_eq!(set.edits[0].observers, ok);
    assert_eq!(set.edits[0].votes, ok);
    assert_eq!(set.edits[0].first.len(), 8);
    assert_eq!(counter(set, "reduce.claims"), ok);
    eprintln!("one site, {n} shards, fan_in 2: {took:?}, {:?}", r.peak);
    r
}

/// What reduce holds for one site claimed by every shard, `fan_in` 2 and
/// one thread, whatever the number of shards (§5.6, landed WP-142 §2):
/// one shard's section at a time; three temporary files (a merge's two
/// inputs and its output); four collapsed records (a run's: two observer
/// records, the file, the claim); the run just finished, folded at once;
/// no unparsed observer.
const ONE_SITE: Peak = Peak { sections: 1, files: 3, records: 4, waiting_runs: 1, unparsed: 0 };

/// §5.6: one site claimed by every observer costs one record at every
/// level, and nothing reduce holds grows with the observers or the runs:
/// the counts of what it held are exact, and the same for a thousand
/// shards and for ten thousand. With `fan_in` 2 they are within §5.6's
/// bounds: at most `fan_in` sections, `fan_in + 1` open files.
#[test]
fn one_site_ten_thousand_shards_fan_in_two() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let small = one_site_run(1_000, none, 1_000).peak;
    let large = one_site_run(10_000, none, 10_000).peak;
    assert_eq!(small, ONE_SITE, "1000 shards");
    assert_eq!(large, ONE_SITE, "10000 shards");
    assert!(large.sections <= 2 && large.files <= 3);
}

/// Every third observer has no shard.
fn every_third(i: usize) -> bool {
    i % 3 == 2
}

/// §5.6: the edit set lists the unparsed observers (§6), 32 bytes each:
/// the one thing reduce holds that grows with the observers. Everything
/// else it holds is as with every shard `ok`.
#[test]
fn unparsed_observers_are_the_one_term_of_the_observers() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let r = one_site_run(3_000, every_third, 2_000);
    assert_eq!(r.edits.unparsed.len(), 1_000);
    assert_eq!(counter(&r.edits, "reduce.unparsed"), 1_000);
    assert_eq!(r.peak, Peak { unparsed: 1_000, ..ONE_SITE });
    assert_eq!(size_of::<Digest>(), 32, "the stated term is 32 bytes an observer");
}

/// The resident reading of the one-site call of ten thousand shards
/// (module documentation): at most 1 MiB of growth, two shards' sections,
/// three files' records and buffers (8 KiB each), the joined state and
/// the edit set, and the allocator's slack. Printed and checked only here:
/// `cargo test -p slip_reduce --test memory -- --ignored --nocapture`.
#[test]
#[ignore = "a resident reading, which a loaded host or a kernel's counters can move"]
fn one_site_ten_thousand_shards_resident() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // The first call of the process warms the allocator and the standard
    // library; it is not read.
    one_site_run(1_000, none, 1_000);
    let (_, growth) = peak_growth(|| one_site_run(10_000, none, 10_000));
    eprintln!("one site, 10000 shards: peak growth {growth:?} KiB");
    match growth {
        Some(g) => assert!(g <= 1024, "one site of 10000 shards grew the peak by {g} KiB"),
        None => eprintln!(
            "the resident high-water mark cannot be measured here: the bound is not checked"
        ),
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

/// A rename-like run of `n` shards on `threads` threads; the resident
/// reading only if `resident`.
fn large_run(n: usize, threads: usize, resident: bool) {
    let store = Lazy { n, make: rename_like, unparsed: none };
    let opts = Options { fan_in: 64, threads, scratch: scratch("large") };
    let start = Instant::now();
    let call = || reduce(&script(), REQUEST, PLAN, 0, &store, &opts).unwrap();
    let (out, growth) = if resident { peak_growth(call) } else { (call(), None) };
    let took = start.elapsed();
    assert_clean(&opts.scratch);
    let set = out.edits;
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.edits.len(), 8 + 12 * n);
    assert_eq!(counter(&set, "reduce.edits.shared"), 8);
    // §5.6: one section per thread at a time, at most a window of runs'
    // states waiting, no unparsed observer.
    let peak = out.peak;
    let window = (WINDOW_PER_THREAD * threads) as u64;
    assert!(peak.sections >= 1 && peak.sections <= threads as u64, "{peak:?}");
    assert!(peak.waiting_runs >= 1 && peak.waiting_runs <= window, "{peak:?}");
    assert_eq!(peak.unparsed, 0);
    eprintln!(
        "rename-like run, {n} shards, {} claims, {threads} threads: {took:?}, {peak:?}, \
         peak growth {growth:?} KiB",
        20 * n
    );
}

/// §5.6: a generated run of two thousand shards, in every `cargo test`.
#[test]
fn rename_like_two_thousand_shards() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    large_run(2_000, 4, false);
}

/// §5.6: the large run of the handback, ten thousand shards and 200,000
/// claims: `cargo test -p slip_reduce --test memory -- --ignored
/// --nocapture`.
#[test]
#[ignore = "the large run of the handback; minutes in a debug build"]
fn rename_like_ten_thousand_shards() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    large_run(10_000, 8, true);
}
