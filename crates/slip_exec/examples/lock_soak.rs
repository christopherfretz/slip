//! A soak of the store's lock and journal (`spec/exec.md` §2.1 "The lock",
//! "The journal"; WP-161) on a real file system:
//!
//! ```text
//! cargo run -p slip_exec --example lock_soak -- <dir> [--rounds <n>] [--procs <p>] [--kills <k>]
//! ```
//!
//! `<p>` processes (4) each open the store `<dir>/store` exclusively
//! `<n>` times (2,000). An opener that holds the lock creates
//! `<dir>/holder` exclusively with its pid, checks that the lock reads its
//! pid, publishes, reads back and removes the journal, removes the holder
//! file and closes the store. A holder file of another live process found
//! there is two holders at once: `DOUBLE`. While they run, the parent kills
//! a random opener `<k>` times (20) and starts another in its place: the
//! leftovers of a kill at any step (a claim, an empty or partial lock, a
//! dead pid's lock, a journal) must never stop the others for good.
//!
//! The last line is `soak: ok` when no opener saw two holders, a lock not
//! its own, or a journal read back wrong, and every opener ended; else
//! `soak: FAILED`. Every count is printed.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use slip_exec::store::{Access, Store, StoreError};

const CHILD: &str = "SLIP_LOCK_SOAK_CHILD";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = None;
    let (mut rounds, mut procs, mut kills) = (2_000u64, 4usize, 20u64);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut num = |name: &str| -> u64 {
            it.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                eprintln!("lock_soak: {name} needs a number");
                std::process::exit(2)
            })
        };
        match a.as_str() {
            "--rounds" => rounds = num("--rounds"),
            "--procs" => procs = usize::try_from(num("--procs")).unwrap_or(4),
            "--kills" => kills = num("--kills"),
            s => dir = Some(PathBuf::from(s)),
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: lock_soak <dir> [--rounds <n>] [--procs <p>] [--kills <k>]");
        return ExitCode::from(2);
    };
    if std::env::var_os(CHILD).is_some() {
        return child(&dir, rounds);
    }
    parent(&dir, rounds, procs, kills)
}

// ---------------------------------------------------------------------------
// An opener

#[derive(Default)]
struct Counts {
    locked: u64,
    busy: u64,
    recovered: u64,
    stale_holder: u64,
    holder_absent: u64,
    double: u64,
    not_mine: u64,
    journal_wrong: u64,
    journal_absent: u64,
    errors: u64,
}

impl Counts {
    /// `COUNTS <pid> <name> <n>…`: the counts so far (the parent keeps
    /// each opener's last, so a killed opener's count until its last
    /// line).
    fn print(&self, out: &mut dyn io::Write, me: u32) {
        let _ = writeln!(
            out,
            "COUNTS {me} locked {} busy {} recovered {} stale_holder {} holder_absent {} \
             double {} not_mine {} journal_wrong {} journal_absent {} errors {}",
            self.locked,
            self.busy,
            self.recovered,
            self.stale_holder,
            self.holder_absent,
            self.double,
            self.not_mine,
            self.journal_wrong,
            self.journal_absent,
            self.errors
        );
        let _ = out.flush();
    }
}

fn alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn child(dir: &Path, rounds: u64) -> ExitCode {
    let me = std::process::id();
    let store_dir = dir.join("store");
    let holder = dir.join("holder");
    let mut c = Counts::default();
    let mut out = io::stdout();
    for i in 0..rounds {
        if i % 25 == 0 {
            c.print(&mut out, me);
        }
        let store = match Store::open(&store_dir, Access::Exclusive) {
            Ok(s) => s,
            Err(StoreError::Busy { .. }) => {
                c.busy += 1;
                continue;
            }
            Err(StoreError::UnfinishedWrite(_)) => {
                // A killed holder's journal: removed as --recover would.
                match Store::open_for_recovery(&store_dir) {
                    Ok(s) => {
                        let _ = s.remove_journal();
                        c.recovered += 1;
                    }
                    Err(StoreError::Busy { .. }) => c.busy += 1,
                    Err(e) => {
                        c.errors += 1;
                        let _ = writeln!(out, "ERROR recover {e}");
                    }
                }
                continue;
            }
            Err(e) => {
                c.errors += 1;
                let _ = writeln!(out, "ERROR open {e}");
                continue;
            }
        };
        c.locked += 1;
        // The lock is this process's.
        match fs::read(store_dir.join("lock")) {
            // `<pid> <nonce>\n` (`spec/exec.md` §2.1 "The lock").
            Ok(t) if t.starts_with(format!("{me} ").as_bytes()) && t.ends_with(b"\n") => {}
            Ok(t) => {
                c.not_mine += 1;
                let _ = writeln!(out, "NOT_MINE {me} read {:?}", String::from_utf8_lossy(&t));
            }
            // The hazard of a name just created: not two holders.
            Err(_) => c.holder_absent += 1,
        }
        let held = hold(&holder, me, &mut c, &mut out);
        journal(&store, me, i, &mut c, &mut out);
        if held {
            let _ = fs::remove_file(&holder);
        }
        drop(store);
    }
    c.print(&mut out, me);
    let _ = writeln!(out, "END {me}");
    let _ = out.flush();
    ExitCode::SUCCESS
}

/// The holder file: made exclusively; another live pid's is two holders at
/// once. Whether this opener made it.
fn hold(holder: &Path, me: u32, c: &mut Counts, out: &mut dyn io::Write) -> bool {
    let mut held = false;
    for _ in 0..3 {
        match fs::OpenOptions::new().write(true).create_new(true).open(holder) {
            Ok(mut f) => {
                let _ = write!(f, "{me}");
                held = true;
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                match fs::read_to_string(holder) {
                    Ok(t) => match t.parse::<u32>() {
                        Ok(p) if p != me && alive(p) => {
                            c.double += 1;
                            let _ = writeln!(out, "DOUBLE {me} found holder {p}");
                            break;
                        }
                        // A killed holder's, or one cut before its
                        // write: removed.
                        _ => {
                            c.stale_holder += 1;
                            let _ = fs::remove_file(holder);
                        }
                    },
                    Err(_) => c.holder_absent += 1,
                }
            }
            Err(e) => {
                c.errors += 1;
                let _ = writeln!(out, "ERROR holder {e}");
                break;
            }
        }
    }
    held
}

/// The journal: published, read back, removed.
fn journal(store: &Store, me: u32, i: u64, c: &mut Counts, out: &mut dyn io::Write) {
    let bytes = format!("journal of {me} round {i}\n").into_bytes();
    match store.publish_journal(&bytes) {
        Ok(()) => {
            match store.read_journal() {
                Ok(Some(b)) if b == bytes => {}
                Ok(Some(b)) => {
                    c.journal_wrong += 1;
                    let _ =
                        writeln!(out, "JOURNAL_WRONG {me} read {:?}", String::from_utf8_lossy(&b));
                }
                Ok(None) => c.journal_absent += 1,
                Err(e) => {
                    c.errors += 1;
                    let _ = writeln!(out, "ERROR read journal {e}");
                }
            }
            let mut removed = store.remove_journal();
            // Removing a name read as absent: once more.
            if removed.is_err() {
                removed = store.remove_journal();
            }
            if let Err(e) = removed {
                c.errors += 1;
                let _ = writeln!(out, "ERROR remove journal {e}");
            }
        }
        Err(e) => {
            c.errors += 1;
            let _ = writeln!(out, "ERROR publish journal {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// The parent

fn spawn(dir: &Path, rounds: u64, tx: &Sender<String>) -> io::Result<Child> {
    // Its name as started: on the workspace mount `/proc/self/exe` can read
    // `… (deleted)` for a binary just linked (its inode number changed,
    // `spec/harness.md` §11.5).
    let exe = std::env::args_os().next().map_or_else(std::env::current_exe, |a| Ok(a.into()))?;
    let start = || {
        Command::new(&exe)
            .arg(dir)
            .args(["--rounds", &rounds.to_string()])
            .env(CHILD, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
    };
    // And the name can read as absent for a moment (`unique-tmp-link`).
    let mut tries = 0;
    let mut c = loop {
        match start() {
            Err(e) if e.kind() == io::ErrorKind::NotFound && tries < 50 => {
                eprintln!("lock_soak: spawn of {}: {e}; again", exe.display());
                tries += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            r => break r?,
        }
    };
    let out = c.stdout.take().expect("piped");
    let tx = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    Ok(c)
}

fn parent(dir: &Path, rounds: u64, procs: usize, kills: u64) -> ExitCode {
    let store_dir = dir.join("store");
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("lock_soak: {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let _ = fs::remove_dir_all(&store_dir);
    let _ = fs::remove_file(dir.join("holder"));
    drop(Store::open(&store_dir, Access::Shared).expect("the store"));
    let t0 = Instant::now();
    let (tx, rx) = channel();
    let mut children: Vec<Child> = Vec::new();
    for _ in 0..procs {
        children.push(spawn(dir, rounds, &tx).expect("spawn"));
    }
    // Kills spread over the run: a pseudo-random opener, a pseudo-random
    // moment (20 to 400 ms apart), while openers are running.
    let mut seed = u64::from(std::process::id()) | 1;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut killed = 0u64;
    while killed < kills {
        std::thread::sleep(Duration::from_millis(20 + next() % 381));
        let live: Vec<usize> =
            (0..children.len()).filter(|&i| matches!(children[i].try_wait(), Ok(None))).collect();
        if live.is_empty() {
            break;
        }
        let i = live[usize::try_from(next()).unwrap_or(0) % live.len()];
        let _ = children[i].kill();
        let _ = children[i].wait();
        killed += 1;
        children.push(spawn(dir, rounds / 4 + 1, &tx).expect("spawn"));
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_hours(1);
    for c in &mut children {
        loop {
            match c.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() > deadline => {
                    let _ = c.kill();
                    let _ = c.wait();
                    break;
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
    let (totals, ended, bad) = tally(&rx);
    let seconds = t0.elapsed().as_secs_f64();
    println!(
        "soak: {} openers ({procs} at a time, {killed} killed, {ended} ended), {seconds:.1}s",
        children.len()
    );
    for (k, v) in &totals {
        println!("  {k} {v}");
    }
    for l in bad.iter().take(20) {
        println!("  {l}");
    }
    let wrong = bad.len() as u64
        + ["double", "not_mine", "journal_wrong", "errors"]
            .iter()
            .map(|k| totals.get(*k).copied().unwrap_or(0))
            .sum::<u64>();
    let expected_ends = children.len() as u64 - killed;
    let _ = fs::remove_dir_all(&store_dir);
    let _ = fs::remove_file(dir.join("holder"));
    if wrong == 0 && ended == expected_ends && totals.get("locked").copied().unwrap_or(0) > 0 {
        println!("soak: ok");
        ExitCode::SUCCESS
    } else {
        println!("soak: FAILED");
        ExitCode::FAILURE
    }
}

/// Each opener's last counts, summed; the openers that ended; the lines
/// that are neither.
fn tally(rx: &Receiver<String>) -> (BTreeMap<String, u64>, u64, Vec<String>) {
    let mut last: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
    let mut ended = 0u64;
    let mut bad = Vec::new();
    for line in rx {
        if let Some(rest) = line.strip_prefix("COUNTS ") {
            let mut words = rest.split_whitespace();
            let pid = words.next().unwrap_or("").to_owned();
            let words: Vec<&str> = words.collect();
            let counts = words
                .chunks(2)
                .filter_map(|kv| match kv {
                    [k, v] => Some(((*k).to_owned(), v.parse::<u64>().unwrap_or(0))),
                    _ => None,
                })
                .collect();
            last.insert(pid, counts);
        } else if line.starts_with("END ") {
            ended += 1;
        } else {
            bad.push(line);
        }
    }
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for counts in last.values() {
        for (k, v) in counts {
            *totals.entry(k.clone()).or_default() += v;
        }
    }
    (totals, ended, bad)
}
