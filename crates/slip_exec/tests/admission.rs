//! `spec/exec.md` §6 with numbers only: [`Admission`] is driven directly,
//! with no process, no clock and no sleep. Each test names the sentence of
//! §6 it covers; the rules beyond the spec are in
//! `spec/landed/exec/WP-009.md`.

use std::collections::HashMap;

use slip_exec::admission::{
    ALONE_AFTER_REQUEUES, Admission, Config, DEFAULT_ESTIMATE_BASE, DEFAULT_ESTIMATE_PER_BYTE,
    Decision, GIB, Pick, SKIP_LIMIT, budget_from_system, cgroup_memory_max, cgroup_v2_path,
    estimate, meminfo_total,
};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin};

const MIB: u64 = 1 << 20;

fn observer(name: &str, size: u64) -> Observer {
    let command = Command {
        directory: b"/".to_vec(),
        argv: vec![b"cc".to_vec(), name.as_bytes().to_vec()],
        env: vec![],
    };
    Observer {
        id: command.observer_id(),
        command,
        language: Language::C,
        main: FileRef { path: format!("/{name}").into_bytes(), size, content: Digest([1; 32]) },
        cost_hint: size,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

/// A config whose estimate is exactly the main file's size in MiB
/// (`base 0`, `per_byte` 1 MiB), so estimates are chosen by `size`.
fn config(budget: u64) -> Config {
    Config {
        budget,
        estimate_base: 0,
        estimate_per_byte: MIB,
        observer_memory: None,
        worker_stack: 16 * MIB,
    }
}

/// An admission over observers whose estimates are `mib[i]` MiB.
fn admission(budget: u64, mib: &[u64]) -> Admission {
    let obs: Vec<Observer> =
        mib.iter().enumerate().map(|(i, &m)| observer(&format!("o{i}"), m)).collect();
    Admission::new(config(budget), &HashMap::new(), &obs)
}

fn pick(a: &mut Admission) -> Option<usize> {
    a.pick().map(|p| p.job)
}

// ---------------------------------------------------------------------------
// Budget

/// §6 "Budget": the smaller of the cgroup limit and total memory, minus the
/// larger of 2 GiB and one tenth.
#[test]
fn s6_budget_smaller_of_cgroup_and_total_minus_reserve() {
    // 16 GiB cgroup on a 64 GiB machine: reserve 2 GiB (> 1.6 GiB).
    assert_eq!(budget_from_system(Some(16 * GIB), 64 * GIB), 14 * GIB);
    // The machine is the smaller one.
    assert_eq!(budget_from_system(Some(64 * GIB), 16 * GIB), 14 * GIB);
    // `max` (no number): total memory alone.
    assert_eq!(budget_from_system(None, 16 * GIB), 14 * GIB);
    // 100 GiB: one tenth (10 GiB) is larger than 2 GiB.
    assert_eq!(budget_from_system(None, 100 * GIB), 90 * GIB);
    // Exactly 20 GiB: both reserves are 2 GiB.
    assert_eq!(budget_from_system(None, 20 * GIB), 18 * GIB);
}

/// §6 "Budget", the cases the formula leaves open (landed): a machine
/// smaller than the 2 GiB reserve keeps the one-tenth reserve; unknown
/// memory is 0 (no admission); a cgroup limit with unknown total is used.
#[test]
fn s6_budget_edge_cases() {
    assert_eq!(budget_from_system(None, GIB), GIB - GIB / 10);
    assert_eq!(budget_from_system(Some(2 * GIB), 64 * GIB), 2 * GIB - (2 * GIB) / 10);
    assert_eq!(budget_from_system(None, 0), 0);
    assert_eq!(budget_from_system(Some(8 * GIB), 0), 6 * GIB);
}

/// §6 "Budget": cgroup v2 `memory.max` when it is a number; `MemTotal`.
#[test]
fn s6_budget_system_readings_parse() {
    assert_eq!(cgroup_memory_max("17179869184\n"), Some(16 * GIB));
    assert_eq!(cgroup_memory_max("max\n"), None);
    assert_eq!(cgroup_memory_max(""), None);
    assert_eq!(cgroup_v2_path("1:name=systemd:/x\n0::/user.slice/a\n"), Some("/user.slice/a"));
    assert_eq!(cgroup_v2_path("1:cpu:/x\n"), None);
    assert_eq!(
        meminfo_total("MemTotal:       16384000 kB\nMemFree: 1 kB\n"),
        Some(16_384_000 * 1024)
    );
    assert_eq!(meminfo_total("MemFree: 1 kB\n"), None);
    assert_eq!(meminfo_total("MemTotal: lots\n"), None);
    // On this (Linux) host the total is known.
    let (_, total) = slip_exec::admission::read_system();
    assert!(total > 0);
}

// ---------------------------------------------------------------------------
// Estimate

/// §6 "Estimate" rule 1: the previous run's peak for the observer × 1.25.
#[test]
fn s6_estimate_from_previous_peak() {
    let o = observer("a", 10);
    let mut previous = HashMap::new();
    previous.insert(o.id, 400 * MIB);
    assert_eq!(estimate(&config(GIB), &previous, &o), 500 * MIB);
    // Rounded down; saturating.
    previous.insert(o.id, 7);
    assert_eq!(estimate(&config(GIB), &previous, &o), 8);
    previous.insert(o.id, u64::MAX);
    assert_eq!(estimate(&config(GIB), &previous, &o), u64::MAX);
    let a = Admission::new(config(GIB), &previous, &[o]);
    assert_eq!(a.estimate(0), u64::MAX);
}

/// §6 "Estimate" rule 2: base plus per-byte times the main file's size,
/// also when the previous peak is 0 ("unknown") or another observer's.
#[test]
fn s6_estimate_from_main_file_size() {
    let o = observer("a", 1000);
    let other = observer("b", 1);
    let c = Config::with_budget(GIB);
    assert_eq!(c.estimate_base, DEFAULT_ESTIMATE_BASE);
    assert_eq!(c.estimate_per_byte, DEFAULT_ESTIMATE_PER_BYTE);
    let want = 512 * MIB + 64 * 1000;
    assert_eq!(estimate(&c, &HashMap::new(), &o), want);
    let mut previous = HashMap::new();
    previous.insert(o.id, 0);
    previous.insert(other.id, 5 * GIB);
    assert_eq!(estimate(&c, &previous, &o), want);
    let huge = observer("h", u64::MAX);
    assert_eq!(estimate(&c, &HashMap::new(), &huge), u64::MAX);
}

// ---------------------------------------------------------------------------
// Reservation and admission

/// §6 "Reservation": `r = max(e, sample)`; `R` is their sum.
#[test]
fn s6_reservation_is_larger_of_estimate_and_sample() {
    let mut a = admission(1000 * MIB, &[100, 100, 100]);
    assert_eq!(pick(&mut a), Some(0));
    assert_eq!(pick(&mut a), Some(1));
    assert_eq!(a.reserved(), 200 * MIB);
    assert!(a.on_samples(&[(0, 30 * MIB), (1, 250 * MIB)]).is_empty());
    assert_eq!(a.reserved(), 100 * MIB + 250 * MIB);
    // A sample for a job that is not running changes nothing.
    assert!(a.on_samples(&[(2, 900 * MIB)]).is_empty());
    assert_eq!(a.reserved(), 350 * MIB);
}

/// §6 "Admission": admitted if `R + e ≤ B` (equality admits).
#[test]
fn s6_admitted_when_it_fits() {
    let mut a = admission(300 * MIB, &[100, 100, 100, 100]);
    assert_eq!(
        a.pick(),
        Some(Pick { job: 0, estimate: 100 * MIB, alone: false }),
        "the first in plan order"
    );
    assert_eq!(pick(&mut a), Some(1));
    assert_eq!(pick(&mut a), Some(2), "R + e = B is admitted");
    assert_eq!(a.peek(), None);
    assert_eq!(pick(&mut a), None, "R + e > B");
    assert_eq!(a.running_len(), 3);
    a.finished(1, false);
    assert_eq!(pick(&mut a), Some(3));
}

/// §6 "Admission": "or if nothing is running": a job larger than `B` runs,
/// by itself.
#[test]
fn s6_admitted_when_nothing_runs() {
    let mut a = admission(100 * MIB, &[500, 1]);
    assert_eq!(pick(&mut a), Some(0));
    assert_eq!(a.peek(), None, "R = 500 MiB > B: nothing else fits");
    a.finished(0, false);
    assert_eq!(pick(&mut a), Some(1));
}

/// §6 "Admission": the first pending job in plan order that is admitted,
/// so a smaller job starts while a larger one waits.
#[test]
fn s6_backfill_smaller_job() {
    let mut a = admission(300 * MIB, &[200, 200, 50, 60]);
    assert_eq!(pick(&mut a), Some(0));
    assert_eq!(a.peek(), Some(2));
    assert_eq!(pick(&mut a), Some(2), "job 1 (200) does not fit beside 200; job 2 (50) does");
    assert_eq!(a.skips(1), 1, "job 1 was passed over");
    assert_eq!(pick(&mut a), None, "260 + 60 > 300, and 260 + 200 > 300");
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 3]);
    a.finished(0, false);
    assert_eq!(pick(&mut a), Some(1), "plan order again once it fits");
}

/// §6 "Admission": a job passed over 64 times blocks all later jobs until
/// it is admitted.
#[test]
fn s6_skip_guard_blocks_after_64() {
    // Job 0 (100) keeps running; job 1 (250) never fits beside it; small
    // jobs (1 MiB each) backfill.
    let mut sizes = vec![100, 250];
    sizes.extend(std::iter::repeat_n(1, 70));
    let mut a = admission(300 * MIB, &sizes);
    assert_eq!(pick(&mut a), Some(0));
    for n in 0..SKIP_LIMIT {
        let j = pick(&mut a).expect("a small job backfills");
        assert_eq!(j, 2 + n as usize);
        assert_eq!(a.skips(1), n + 1);
        a.finished(j, false);
    }
    assert_eq!(a.skips(1), SKIP_LIMIT);
    assert_eq!(a.peek(), None, "job 1 now blocks the small jobs");
    assert_eq!(pick(&mut a), None);
    a.finished(0, false);
    assert_eq!(pick(&mut a), Some(1), "admitted once nothing else runs");
    assert_eq!(pick(&mut a), Some(2 + SKIP_LIMIT as usize), "the rest follow");
}

/// §6 with `--memory 0`: no admission (plan order up to `-j`, which is the
/// coordinator's), no caps, no pressure.
#[test]
fn s6_budget_zero_disables_admission_and_caps() {
    let mut c = config(0);
    c.observer_memory = Some(MIB);
    let obs: Vec<Observer> = (0..4).map(|i| observer(&format!("z{i}"), 1 << 40)).collect();
    let mut a = Admission::new(c, &HashMap::new(), &obs);
    assert_eq!(a.rss_cap(), None);
    assert_eq!(a.address_space_cap(), None);
    for i in 0..4 {
        assert_eq!(pick(&mut a), Some(i));
    }
    let huge: Vec<(usize, u64)> = (0..4).map(|i| (i, u64::MAX / 8)).collect();
    assert!(a.on_samples(&huge).is_empty());
}

// ---------------------------------------------------------------------------
// Caps

/// §6 "Caps": the resident-size cap is `--observer-memory`, default `B`;
/// the address-space cap is twice it plus the worker stack plus 1 GiB.
#[test]
fn s6_caps_values() {
    let a = admission(4 * GIB, &[1]);
    assert_eq!(a.rss_cap(), Some(4 * GIB));
    assert_eq!(a.address_space_cap(), Some(8 * GIB + 16 * MIB + GIB));
    let mut c = config(4 * GIB);
    c.observer_memory = Some(GIB);
    c.worker_stack = GIB;
    let a = Admission::new(c, &HashMap::new(), &[observer("a", 1)]);
    assert_eq!(a.rss_cap(), Some(GIB));
    assert_eq!(a.address_space_cap(), Some(4 * GIB));
    c.observer_memory = Some(u64::MAX);
    let a = Admission::new(c, &HashMap::new(), &[observer("a", 1)]);
    assert_eq!(a.address_space_cap(), Some(u64::MAX), "saturating");
}

/// §6 "Caps": a job whose sample exceeds the cap is killed; equal is not
/// over; 0 ("unknown") never is. §6 "Retry": alone, it fails.
#[test]
fn s6_cap_alone_fails() {
    let mut c = config(GIB);
    c.observer_memory = Some(100 * MIB);
    let mut a = Admission::new(c, &HashMap::new(), &[observer("a", 1)]);
    assert_eq!(pick(&mut a), Some(0));
    assert!(a.on_samples(&[(0, 0)]).is_empty());
    assert!(a.on_samples(&[(0, 100 * MIB)]).is_empty(), "equal to the cap is not over it");
    assert_eq!(a.on_samples(&[(0, 100 * MIB + 1)]), [Decision::KillAtCap { job: 0, retry: false }]);
    assert!(a.is_killing(0));
    assert!(a.on_samples(&[(0, GIB)]).is_empty(), "decided once");
    assert_eq!(a.reserved(), GIB, "it reserves memory until it is gone");
    a.finished(0, true);
    assert_eq!(a.pending_len(), 0, "failed: not requeued, whatever the coordinator asks");
    assert_eq!(a.running_len(), 0);
}

/// §6 "Retry": killed at its cap while others run, retried once, alone
/// (nothing else is admitted while it runs, and it waits for the others);
/// killed at its cap again (alone), it fails.
#[test]
fn s6_cap_retry_alone_then_fail() {
    let mut c = config(GIB);
    c.observer_memory = Some(100 * MIB);
    let obs: Vec<Observer> = (0..3).map(|i| observer(&format!("c{i}"), 10)).collect();
    let mut a = Admission::new(c, &HashMap::new(), &obs);
    assert_eq!(pick(&mut a), Some(0));
    assert_eq!(pick(&mut a), Some(1));
    assert_eq!(
        a.on_samples(&[(0, 20 * MIB), (1, 200 * MIB)]),
        [Decision::KillAtCap { job: 1, retry: true }]
    );
    a.finished(1, false);
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 2], "back at the head");
    assert!(a.runs_alone(1));
    assert_eq!(a.peek(), None, "job 1 must run alone and job 0 runs: it blocks job 2");
    a.finished(0, false);
    assert_eq!(a.pick(), Some(Pick { job: 1, estimate: 10 * MIB, alone: true }));
    assert_eq!(a.peek(), None, "nothing is admitted beside a job running alone");
    assert_eq!(a.on_samples(&[(1, 200 * MIB)]), [Decision::KillAtCap { job: 1, retry: false }]);
    a.finished(1, false);
    assert_eq!(a.pending().collect::<Vec<_>>(), [2]);
    assert!(!a.runs_alone(1));
    assert_eq!(pick(&mut a), Some(2));
}

/// §6 "Retry": "retried once": a job already retried for its cap is not
/// retried again even if it is killed beside others.
#[test]
fn s6_cap_retry_only_once() {
    let mut c = config(GIB);
    c.observer_memory = Some(100 * MIB);
    let obs: Vec<Observer> = (0..2).map(|i| observer(&format!("r{i}"), 10)).collect();
    let mut a = Admission::new(c, &HashMap::new(), &obs);
    pick(&mut a);
    pick(&mut a);
    assert_eq!(
        a.on_samples(&[(0, 200 * MIB), (1, 200 * MIB)]),
        [Decision::KillAtCap { job: 0, retry: true }, Decision::KillAtCap { job: 1, retry: true }],
        "both were killed while another ran"
    );
    a.finished(0, false);
    a.finished(1, false);
    // Both must run alone: one at a time.
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 0]);
    let first = a.pick().unwrap();
    assert!(first.alone);
    assert_eq!(a.peek(), None);
    a.finished(first.job, false);
    let second = a.pick().unwrap();
    assert!(second.alone);
    assert_eq!(
        a.on_samples(&[(second.job, 200 * MIB)]),
        [Decision::KillAtCap { job: second.job, retry: false }]
    );
}

// ---------------------------------------------------------------------------
// Pressure

/// §6 "Pressure": samples summing to more than `B` with more than one job
/// running: the most recently started job is killed and put back at the
/// head with `e = 2 ×` its last sample; it is not a failure (requeued, not
/// failed).
#[test]
fn s6_pressure_kills_newest_and_requeues_at_head() {
    let mut a = admission(1000 * MIB, &[100, 100, 100, 100]);
    for i in 0..3 {
        assert_eq!(pick(&mut a), Some(i));
    }
    // Sum 1000: not more than B.
    assert!(a.on_samples(&[(0, 400 * MIB), (1, 300 * MIB), (2, 300 * MIB)]).is_empty());
    // Sum 1001: job 2 started last.
    assert_eq!(
        a.on_samples(&[(0, 400 * MIB), (1, 300 * MIB), (2, 300 * MIB + 1)]),
        [Decision::KillForPressure { job: 2, estimate: 600 * MIB + 2 }]
    );
    // Being killed, it no longer counts: the sum of the others is 700.
    assert!(a.on_samples(&[(0, 400 * MIB), (1, 300 * MIB), (2, 900 * MIB)]).is_empty());
    a.finished(2, false);
    assert_eq!(a.pending().collect::<Vec<_>>(), [2, 3], "back at the head");
    assert_eq!(a.estimate(2), 1800 * MIB, "2 × its last sample");
    assert_eq!(a.requeues(2), 1);
    assert!(!a.runs_alone(2));
}

/// §6 "Pressure": "most recently started" is start order, not plan order
/// (a backfilled job started later than a requeued one).
#[test]
fn s6_pressure_newest_by_start_order() {
    let mut a = admission(1000 * MIB, &[100, 100, 100]);
    assert_eq!(pick(&mut a), Some(0));
    assert_eq!(pick(&mut a), Some(1));
    a.finished(0, true); // a crash retry: back at the head
    assert_eq!(pick(&mut a), Some(0), "job 0 restarts after job 1");
    assert_eq!(
        a.on_samples(&[(1, 600 * MIB), (0, 600 * MIB)]),
        [Decision::KillForPressure { job: 0, estimate: 1200 * MIB }]
    );
}

/// §6 "Pressure": "while more than one job is running": one job over `B`
/// is not killed for pressure (only its cap applies).
#[test]
fn s6_pressure_needs_two_running() {
    let mut c = config(1000 * MIB);
    c.observer_memory = Some(10 * GIB);
    let mut a = Admission::new(c, &HashMap::new(), &[observer("a", 10), observer("b", 10)]);
    pick(&mut a);
    assert!(a.on_samples(&[(0, 5 * GIB)]).is_empty());
}

/// §6 "Pressure": a job requeued this way three times runs alone from then
/// on (each requeue; the third makes it alone).
#[test]
fn s6_pressure_three_requeues_run_alone() {
    let mut a = admission(1000 * MIB, &[10, 10, 10]);
    pick(&mut a);
    for n in 1..=ALONE_AFTER_REQUEUES {
        let j = pick(&mut a).expect("job 1 is admitted beside job 0");
        assert_eq!(j, 1);
        assert_eq!(
            a.on_samples(&[(0, 600 * MIB), (1, 450 * MIB)]),
            [Decision::KillForPressure { job: 1, estimate: 900 * MIB }]
        );
        a.finished(1, false);
        assert_eq!(a.requeues(1), n);
        assert_eq!(a.runs_alone(1), n == ALONE_AFTER_REQUEUES);
        // e = 900: with job 0 reserving 600 it no longer fits; reset it to
        // fit for the next round by lowering job 0's sample.
        assert!(a.on_samples(&[(0, 50 * MIB)]).is_empty());
        if n < ALONE_AFTER_REQUEUES {
            // 50 + 900 ≤ 1000.
            assert_eq!(a.peek(), Some(1));
        }
    }
    assert_eq!(a.peek(), None, "alone: waits for job 0, and blocks job 2");
    a.finished(0, false);
    assert_eq!(a.pick(), Some(Pick { job: 1, estimate: 900 * MIB, alone: true }));
    assert_eq!(a.peek(), None);
    a.finished(1, true);
    assert!(a.runs_alone(1), "alone from then on: also for a crash retry");
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 2]);
}

/// §6 "Pressure" and "Caps" in one round: a job over its cap is killed for
/// that, and only the jobs not being killed count towards pressure.
#[test]
fn s6_cap_and_pressure_same_round() {
    let mut c = config(1000 * MIB);
    c.observer_memory = Some(600 * MIB);
    let obs: Vec<Observer> = (0..3).map(|i| observer(&format!("p{i}"), 10)).collect();
    let mut a = Admission::new(c, &HashMap::new(), &obs);
    for _ in 0..3 {
        pick(&mut a);
    }
    // Job 0 is over its cap; 1 + 2 = 950 ≤ 1000: no pressure.
    assert_eq!(
        a.on_samples(&[(0, 700 * MIB), (1, 500 * MIB), (2, 450 * MIB)]),
        [Decision::KillAtCap { job: 0, retry: true }]
    );
    // Then 1 + 2 = 1001: job 2 is the newest.
    assert_eq!(
        a.on_samples(&[(1, 551 * MIB), (2, 450 * MIB)]),
        [Decision::KillForPressure { job: 2, estimate: 900 * MIB }]
    );
}

/// §5.4 retry (coordinator) and §6: `finished(job, true)` requeues at the
/// head; [`Admission::stopping`] takes a job out of the decisions.
#[test]
fn s6_finished_retry_and_stopping() {
    let mut a = admission(1000 * MIB, &[10, 10, 10]);
    pick(&mut a);
    pick(&mut a);
    a.stopping(1);
    assert!(a.on_samples(&[(0, 600 * MIB), (1, 600 * MIB)]).is_empty(), "job 1 is stopping");
    a.finished(1, true);
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 2]);
    a.finished(0, false);
    a.finished(7, true); // unknown: no effect
    assert_eq!(a.pending().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(a.running_len(), 0);
}
