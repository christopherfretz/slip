//! The cost of the map shard's negative inputs (work package WP-117,
//! `spec/landed/verify/WP-117.md` "Costs"): not part of the gate.

use std::time::Instant;

use slip_cli::worker::map_observer;
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin};

/// The length of `n` as an unsigned LEB128 varint (`spec/model.md` §3).
fn uv(n: usize) -> usize {
    let mut n = n;
    let mut k = 1;
    while n >= 0x80 {
        n >>= 7;
        k += 1;
    }
    k
}

/// The peak resident memory of this process, in KiB (`VmHWM`).
fn peak_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        })
        .unwrap_or(0)
}

/// The three encodings the section could have, in bytes of the section's
/// body: each path whole; front-coded (the one chosen); by directory (each
/// parent once, then its names).
fn sizes(paths: &[Vec<u8>]) -> (usize, usize, usize) {
    let flat = uv(paths.len()) + paths.iter().map(|p| uv(p.len()) + p.len()).sum::<usize>();
    let mut front = uv(paths.len());
    let mut prev: &[u8] = &[];
    for p in paths {
        let shared = prev.iter().zip(p).take_while(|(a, b)| a == b).count();
        front += uv(shared) + uv(p.len() - shared) + p.len() - shared;
        prev = p;
    }
    let mut groups: Vec<(&[u8], Vec<&[u8]>)> = Vec::new();
    for p in paths {
        let cut = p.iter().rposition(|&b| b == b'/').unwrap_or(0);
        let (d, n) = (&p[..cut], &p[cut + 1..]);
        match groups.iter_mut().find(|(g, _)| *g == d) {
            Some((_, names)) => names.push(n),
            None => groups.push((d, vec![n])),
        }
    }
    let grouped = uv(groups.len())
        + groups
            .iter()
            .map(|(d, ns)| {
                uv(d.len())
                    + d.len()
                    + uv(ns.len())
                    + ns.iter().map(|n| uv(n.len()) + n.len()).sum::<usize>()
            })
            .sum::<usize>();
    (flat, front, grouped)
}

/// The map task on one unit, given by `SLIP_WP117_DIR` (the working
/// directory) and `SLIP_WP117_ARGV` (the command, arguments separated by
/// `\x1f`), with Kiln's lookup log and the negatives on
/// (`SLIP_WP117_LOG=on`) or off (`off`). Prints wall time, peak memory,
/// the shard's size, and the section's entries and bytes in each encoding.
/// One unit and one mode per process, so that the peak is the run's.
#[test]
#[ignore = "a measurement: run by hand on a corpus unit"]
fn measure_negatives_on_a_unit() {
    let dir = std::env::var("SLIP_WP117_DIR").expect("SLIP_WP117_DIR");
    let argv = std::env::var("SLIP_WP117_ARGV").expect("SLIP_WP117_ARGV");
    let on = match std::env::var("SLIP_WP117_LOG").as_deref() {
        Ok("on") => true,
        Ok("off") => false,
        _ => panic!("SLIP_WP117_LOG is on or off"),
    };
    let observer = Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.into_bytes(),
            argv: argv.split('\x1f').map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let before = peak_kib();
    let t0 = Instant::now();
    let shard = map_observer(&observer, 0, on).expect("the map task");
    let wall = t0.elapsed();
    let peak = peak_kib();
    let total = shard.to_bytes().len();
    let inputs = shard.inputs.as_ref().map_or(0, Vec::len);
    let mut line = format!(
        "WP117 log={} status={:?} wall={:.2}s peak={} KiB (before {} KiB) shard={} B inputs={}",
        if on { "on" } else { "off" },
        shard.outcome.status,
        wall.as_secs_f64(),
        peak,
        before,
        total,
        inputs,
    );
    if let Some(n) = &shard.negatives {
        let without = slip_model::Shard { negatives: None, ..shard.clone() }.to_bytes().len();
        let (flat, front, grouped) = sizes(n);
        let dirs = n
            .iter()
            .map(|p| &p[..p.iter().rposition(|&b| b == b'/').unwrap_or(0)])
            .collect::<std::collections::HashSet<_>>()
            .len();
        line.push_str(&format!(
            " negatives={} dirs={dirs} section={} B (in the shard) flat={flat} front={front} grouped={grouped}",
            n.len(),
            total - without,
        ));
    }
    println!("{line}");
}
