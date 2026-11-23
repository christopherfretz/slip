//! `spec/exec.md` §2: the store. Opening, the layout, publishing, locking
//! and cleaning, plus the rules of §2.1. The only
//! child processes are this test binary re-executed as a lock holder and
//! `true`; each is killed and reaped on every path.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use slip_exec::store::{Access, Store, StoreError, cleanup_on_signal};
use slip_model::{
    Command as Cmd, Configuration, Digest, End, FileRef, Job, Language, Observer, Origin, Outcome,
    Phase, Plan, Run, RunRecord, Shard, Status,
};

// ---------------------------------------------------------------------------
// Helpers

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("slip-exec-store-{}-{tag}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn plan(tag: &str) -> Plan {
    let command =
        Cmd { directory: b"/".to_vec(), argv: vec![b"cc".to_vec(), tag.into()], env: vec![] };
    Plan {
        tool_slip: "s".into(),
        tool_kiln: "k".into(),
        configurations: vec![Configuration {
            name: "a".into(),
            database: FileRef { path: b"/d".to_vec(), size: 1, content: d(1) },
        }],
        observers: vec![Observer {
            id: command.observer_id(),
            command,
            language: Language::C,
            main: FileRef { path: b"/m".to_vec(), size: 2, content: d(2) },
            cost_hint: 2,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }],
        excluded: vec![],
    }
}

fn shard(observer: Digest, phase: Phase, round: u64) -> Shard {
    Shard {
        observer,
        phase,
        round,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: vec![],
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        negatives: None,
    }
}

fn run(plan: Digest, phase: Phase, round: u64, started: u64, jobs: usize) -> RunRecord {
    RunRecord {
        run: Run {
            tool_slip: "s".into(),
            tool_kiln: "k".into(),
            plan,
            phase,
            round,
            started_unix_ns: started,
            wall_ns: 1,
            cores: 1,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 0,
        },
        jobs: (0..jobs)
            .map(|i| Job {
                observer: d(i as u8),
                attempt: 1,
                end: End::Completed,
                shard: None,
                queued_ns: 0,
                wall_ns: 0,
                cpu_ns: 0,
                peak_rss: 0,
                estimate: 0,
            })
            .collect(),
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = match fs::read_dir(dir) {
        Ok(e) => e.map(|e| e.unwrap().file_name().into_string().unwrap()).collect(),
        Err(_) => vec![],
    };
    v.sort();
    v
}

/// The pid of a process that has exited and been reaped.
fn dead_pid() -> u32 {
    let mut c = Command::new("true").spawn().unwrap();
    let pid = c.id();
    c.wait().unwrap();
    pid
}

// ---------------------------------------------------------------------------
// Opening (§2 "Opening")

#[test]
fn s2_open_initialises_absent_directory() {
    let s = scratch("absent");
    let root = s.join("a").join("b");
    let store = Store::open(&root, Access::Shared).unwrap();
    assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1\n");
    assert_eq!(names(&root), ["VERSION", "declared", "logs", "plans", "runs", "shards", "tmp"]);
    assert!(names(&root.join("tmp")).is_empty());
    assert_eq!(store.root(), root);
}

#[test]
fn s2_open_initialises_empty_directory() {
    let s = scratch("empty");
    let store = Store::open(&s, Access::Exclusive).unwrap();
    assert_eq!(fs::read_to_string(s.join("VERSION")).unwrap(), "1\n");
    drop(store);
    // Reopening an initialised store works, in either mode.
    drop(Store::open(&s, Access::Shared).unwrap());
    drop(Store::open(&s, Access::Exclusive).unwrap());
}

#[test]
fn s2_open_refuses_unrecognised_directory() {
    let s = scratch("unrec");
    fs::write(s.join("something"), "x").unwrap();
    for access in [Access::Shared, Access::Exclusive] {
        match Store::open(&s, access) {
            Err(StoreError::Unrecognised(p)) => assert_eq!(p, *s),
            other => panic!("expected Unrecognised, got {other:?}"),
        }
    }
    // Nothing was written into it.
    assert_eq!(names(&s), ["something"]);
    // A regular file is not a store either.
    let f = s.join("something");
    assert!(matches!(Store::open(&f, Access::Shared), Err(StoreError::Unrecognised(_))));
}

#[test]
fn s2_open_refuses_other_version() {
    let s = scratch("version");
    for text in ["2\n", "1", "1\n\n", ""] {
        fs::write(s.join("VERSION"), text).unwrap();
        match Store::open(&s, Access::Shared) {
            Err(StoreError::Version(v)) => assert_eq!(v, text),
            other => panic!("expected Version for {text:?}, got {other:?}"),
        }
    }
    assert_eq!(names(&s), ["VERSION"]);
}

// §2.1 "Root": the root is absolute.
#[test]
fn s2_root_is_absolute() {
    let s = scratch("relative");
    // A relative path, resolved against the current directory.
    let cwd = std::env::current_dir().unwrap();
    let rel = pathdiff(&s, &cwd);
    let store = Store::open(&rel, Access::Shared).unwrap();
    assert!(store.root().is_absolute());
    assert_eq!(fs::canonicalize(store.root()).unwrap(), fs::canonicalize(&*s).unwrap());
}

/// `to` relative to `from` (both absolute), with `..` components.
fn pathdiff(to: &Path, from: &Path) -> PathBuf {
    let to: Vec<_> = to.components().collect();
    let from: Vec<_> = from.components().collect();
    let common = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to[common..] {
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Layout and publishing (§2 layout, "Publishing")

#[test]
fn s2_layout_paths() {
    let s = scratch("layout");
    let store = Store::open(&s, Access::Shared).unwrap();
    let root = store.root().to_path_buf();
    let p = plan("x").to_bytes();
    let plan_id = store.publish_plan(&p).unwrap();
    assert_eq!(plan_id, slip_model::formats::content_id(&p).unwrap());
    let ph = plan_id.to_string();
    assert_eq!(store.plan_path(&plan_id), root.join(format!("plans/{ph}.plan")));
    assert_eq!(fs::read(root.join(format!("plans/{ph}.plan"))).unwrap(), p);

    let obs = d(0xab);
    let sh = shard(obs, Phase::Verify, 3).to_bytes();
    let shard_id = store.publish_shard(&plan_id, Phase::Verify, 3, &obs, &sh).unwrap();
    assert_eq!(shard_id, slip_model::formats::content_id(&sh).unwrap());
    let shard_file = root.join(format!("shards/{ph}/verify.3/{obs}.shard"));
    assert_eq!(store.shard_path(&plan_id, Phase::Verify, 3, &obs), shard_file);
    assert_eq!(fs::read(&shard_file).unwrap(), sh);
    let sh0 = shard(obs, Phase::Map, 0).to_bytes();
    store.publish_shard(&plan_id, Phase::Map, 0, &obs, &sh0).unwrap();
    assert!(root.join(format!("shards/{ph}/map.0/{obs}.shard")).is_file());

    let r = run(plan_id, Phase::Map, 7, 1234, 1).to_bytes();
    let run_file = store.publish_run(&plan_id, Phase::Map, 7, 1234, &r).unwrap();
    assert_eq!(run_file, root.join(format!("runs/{ph}/map.7.1234.run")));
    assert_eq!(store.run_path(&plan_id, Phase::Map, 7, 1234), run_file);
    assert_eq!(fs::read(&run_file).unwrap(), r);

    let log = store.log_path(&plan_id, Phase::Map, 0, &obs, 2).unwrap();
    assert_eq!(log, root.join(format!("logs/{ph}/map.0/{obs}.2.log")));
    assert!(log.parent().unwrap().is_dir(), "log_path creates the parent directories");
    assert!(!log.exists(), "log_path does not create the file");
    fs::write(&log, "stderr").unwrap();
    store.remove_log(&plan_id, Phase::Map, 0, &obs, 2).unwrap();
    assert!(!log.exists());
    // Removing an absent log is not an error.
    store.remove_log(&plan_id, Phase::Map, 0, &obs, 2).unwrap();

    // Reading back.
    assert_eq!(store.read_plan(&plan_id).unwrap(), Some(p));
    assert_eq!(store.read_plan(&d(9)).unwrap(), None);
    assert_eq!(store.read_shard(&plan_id, Phase::Verify, 3, &obs).unwrap(), Some(sh));
    assert_eq!(store.read_shard(&plan_id, Phase::Verify, 4, &obs).unwrap(), None);

    // Nothing is left in tmp/.
    assert!(names(&root.join("tmp")).is_empty());
}

// §2.1 "Publishing validates": publish_* validate what they publish.
#[test]
fn s2_publish_refuses_invalid_files() {
    let s = scratch("invalid");
    let store = Store::open(&s, Access::Shared).unwrap();
    let obs = d(1);
    let sh = shard(obs, Phase::Map, 0).to_bytes();
    // Not a plan: a shard, garbage, a corrupted plan.
    assert!(matches!(store.publish_plan(&sh), Err(StoreError::Invalid(_))));
    assert!(matches!(store.publish_plan(b"nope"), Err(StoreError::Invalid(_))));
    let mut p = plan("x").to_bytes();
    let n = p.len();
    p[n - 1] ^= 1;
    assert!(matches!(store.publish_plan(&p), Err(StoreError::Invalid(_))));
    // A shard for another observer, phase or round than the path names.
    assert!(matches!(
        store.publish_shard(&d(0), Phase::Map, 0, &d(2), &sh),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.publish_shard(&d(0), Phase::Verify, 0, &obs, &sh),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store.publish_shard(&d(0), Phase::Map, 1, &obs, &sh),
        Err(StoreError::Invalid(_))
    ));
    // A run record for another plan, phase, round or start time.
    let r = run(d(5), Phase::Map, 0, 10, 0).to_bytes();
    assert!(matches!(store.publish_run(&d(6), Phase::Map, 0, 10, &r), Err(StoreError::Invalid(_))));
    assert!(matches!(
        store.publish_run(&d(5), Phase::Verify, 0, 10, &r),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(store.publish_run(&d(5), Phase::Map, 0, 11, &r), Err(StoreError::Invalid(_))));
    // Under another round (`spec/exec.md` §2): the record names round 0.
    assert!(matches!(store.publish_run(&d(5), Phase::Map, 3, 10, &r), Err(StoreError::Invalid(_))));
    let r3 = run(d(5), Phase::Map, 3, 10, 0).to_bytes();
    assert!(matches!(
        store.publish_run(&d(5), Phase::Map, 0, 10, &r3),
        Err(StoreError::Invalid(_))
    ));
    assert!(names(&s.join("plans")).is_empty());
    assert!(names(&s.join("shards")).is_empty());
    assert!(names(&s.join("runs")).is_empty());
    assert!(names(&s.join("tmp")).is_empty());
}

// §2 `declared/`, §2.1 "Publishing validates": a valid `DECL` is published
// under its content id and read back; anything else is `Invalid` and
// nothing is written; a store initialised before `declared/` existed gets
// the directory from the first publish.
#[test]
fn s2_declared_publish_read_and_refuse() {
    use slip_model::declared::Declared;
    let s = scratch("declared");
    let store = Store::open(&s, Access::Shared).unwrap();
    let none = Declared::none().to_bytes();
    let other = Declared::new(vec![], vec![d(3)], vec![], vec![d(4)]).unwrap().to_bytes();
    let id = store.publish_declared(&none).unwrap();
    assert_eq!(id, slip_model::formats::content_id(&none).unwrap());
    assert_eq!(store.declared_path(&id), s.join(format!("declared/{id}.decl")));
    assert_eq!(fs::read(store.declared_path(&id)).unwrap(), none);
    assert_eq!(store.read_declared(&id).unwrap(), Some(none.clone()));
    // Another file, another name; publishing again replaces with the same bytes.
    let id2 = store.publish_declared(&other).unwrap();
    assert_ne!(id, id2);
    assert_eq!(store.publish_declared(&other).unwrap(), id2);
    assert_eq!(store.read_declared(&id2).unwrap(), Some(other.clone()));
    assert_eq!(store.read_declared(&d(9)).unwrap(), None);
    assert_eq!(names(&s.join("declared")).len(), 2);
    // Refused: a shard (another kind), a plan, garbage, a corrupted DECL.
    let mut bad = none.clone();
    let n = bad.len();
    bad[n - 1] ^= 1;
    let sh = shard(d(1), Phase::Map, 0).to_bytes();
    for bytes in [&sh[..], &plan("x").to_bytes()[..], b"nope", &bad[..], &[][..]] {
        assert!(matches!(store.publish_declared(bytes), Err(StoreError::Invalid(_))), "{bytes:?}");
    }
    assert_eq!(names(&s.join("declared")).len(), 2);
    assert!(names(&s.join("tmp")).is_empty());
    drop(store);

    // A store initialised without `declared/`.
    let s = scratch("declared-old");
    drop(Store::open(&s, Access::Shared).unwrap());
    fs::remove_dir(s.join("declared")).unwrap();
    let store = Store::open(&s, Access::Shared).unwrap();
    assert_eq!(store.read_declared(&id).unwrap(), None);
    assert_eq!(store.publish_declared(&none).unwrap(), id);
    assert_eq!(store.read_declared(&id).unwrap(), Some(none));
}

#[test]
fn s2_publish_is_atomic_and_replaces() {
    let s = scratch("atomic");
    let store = Arc::new(Store::open(&s, Access::Shared).unwrap());
    let plan_id = d(0x44);
    // Two different files for the same name, of different sizes.
    let small = run(plan_id, Phase::Map, 0, 99, 1).to_bytes();
    let large = run(plan_id, Phase::Map, 0, 99, 200).to_bytes();
    assert!(large.len() > small.len() * 10);
    let path = store.publish_run(&plan_id, Phase::Map, 0, 99, &small).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let (stop, path, small, large) = (stop.clone(), path.clone(), small.clone(), large.clone());
        std::thread::spawn(move || {
            let mut reads = 0u64;
            while !stop.load(Ordering::SeqCst) {
                let got = fs::read(&path).unwrap();
                assert!(
                    got == small || got == large,
                    "a reader saw a partial file ({} bytes)",
                    got.len()
                );
                reads += 1;
            }
            reads
        })
    };
    for i in 0..300 {
        let bytes = if i % 2 == 0 { &large } else { &small };
        store.publish_run(&plan_id, Phase::Map, 0, 99, bytes).unwrap();
    }
    stop.store(true, Ordering::SeqCst);
    assert!(reader.join().unwrap() > 0);
    // The last write (i = 299, small) replaced the earlier ones.
    assert_eq!(fs::read(&path).unwrap(), small);
    assert!(names(&s.join("tmp")).is_empty());
}

// ---------------------------------------------------------------------------
// find_plan and latest_run

#[test]
fn s2_find_plan_by_prefix() {
    let s = scratch("find");
    let store = Store::open(&s, Access::Shared).unwrap();
    let mut ids: Vec<Digest> =
        (0..20).map(|i| store.publish_plan(&plan(&format!("p{i}")).to_bytes()).unwrap()).collect();
    ids.sort();
    assert_eq!(store.find_plan("").unwrap(), ids);
    for id in &ids {
        let hex = id.to_string();
        assert_eq!(store.find_plan(&hex).unwrap(), vec![*id]);
        let one: Vec<Digest> =
            ids.iter().filter(|x| x.to_string().starts_with(&hex[..1])).copied().collect();
        assert_eq!(store.find_plan(&hex[..1]).unwrap(), one);
    }
    // Not lower-case hex, or too long: nothing.
    let some_letter = ids
        .iter()
        .map(ToString::to_string)
        .find(|h| h.bytes().any(|c| c.is_ascii_lowercase()))
        .unwrap();
    assert!(store.find_plan(&some_letter.to_uppercase()).unwrap().is_empty());
    assert!(store.find_plan("A").unwrap().is_empty());
    assert!(store.find_plan("xyz").unwrap().is_empty());
    assert!(store.find_plan(&format!("{}0", ids[0])).unwrap().is_empty());
    // Files that are not plans are ignored.
    fs::write(s.join("plans").join("junk.plan"), "x").unwrap();
    fs::write(s.join("plans").join(format!("{}.txt", d(1))), "x").unwrap();
    assert_eq!(store.find_plan("").unwrap(), ids);
    // An empty store has none.
    let e = scratch("find-empty");
    assert!(Store::open(&e, Access::Shared).unwrap().find_plan("").unwrap().is_empty());
}

#[test]
fn s2_latest_run_any_round() {
    let s = scratch("latest");
    let store = Store::open(&s, Access::Shared).unwrap();
    let p = d(7);
    assert_eq!(store.latest_run(&p, Phase::Map).unwrap(), None);
    let oldest = run(p, Phase::Map, 5, 100, 1).to_bytes();
    let newest = run(p, Phase::Map, 0, 300, 2).to_bytes();
    let middle = run(p, Phase::Map, 9, 200, 3).to_bytes();
    let verify = run(p, Phase::Verify, 0, 900, 4).to_bytes();
    store.publish_run(&p, Phase::Map, 5, 100, &oldest).unwrap();
    store.publish_run(&p, Phase::Map, 0, 300, &newest).unwrap();
    store.publish_run(&p, Phase::Map, 9, 200, &middle).unwrap();
    store.publish_run(&p, Phase::Verify, 0, 900, &verify).unwrap();
    // Greatest started_unix_ns, whatever the round; numeric, not textual.
    assert_eq!(store.latest_run(&p, Phase::Map).unwrap(), Some(newest));
    assert_eq!(store.latest_run(&p, Phase::Verify).unwrap(), Some(verify));
    let later = run(p, Phase::Map, 1, 1000, 5).to_bytes();
    store.publish_run(&p, Phase::Map, 1, 1000, &later).unwrap();
    assert_eq!(store.latest_run(&p, Phase::Map).unwrap(), Some(later.clone()));
    // Names that are not run records are ignored.
    fs::write(s.join("runs").join(p.to_string()).join("map.0.99999.run.bak"), "x").unwrap();
    fs::write(s.join("runs").join(p.to_string()).join("map.0.099999.run"), "x").unwrap();
    assert_eq!(store.latest_run(&p, Phase::Map).unwrap(), Some(later));
    assert_eq!(store.latest_run(&d(8), Phase::Map).unwrap(), None);
}

// ---------------------------------------------------------------------------
// Locking (§2 "Locking")

/// Re-executed as the lock holder: opens the store named by the variable
/// exclusively, prints `locked`, and holds it until its stdin ends.
const HOLDER: &str = "SLIP_EXEC_STORE_LOCK_HOLDER";

#[test]
fn lock_holder() {
    let Some(dir) = std::env::var_os(HOLDER) else { return };
    let store = Store::open(Path::new(&dir), Access::Exclusive).unwrap();
    println!("locked {}", std::process::id());
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    drop(store);
    println!("released");
    std::io::stdout().flush().unwrap();
}

/// A child process holding the lock; killed and reaped on drop.
struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// How long the lock holder may take to say `locked` or `released`, and to
/// exit (a loaded machine).
const HOLDER_DEADLINE: Duration = Duration::from_mins(1);

/// The holder's standard output, one line per message, read on a thread
/// so that every wait for it is bounded.
type Lines = Receiver<String>;

fn hold_lock(dir: &Path) -> (Holder, Lines) {
    let mut c = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder", "--nocapture", "--test-threads=1"])
        .env(HOLDER, dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = c.stdout.take().unwrap();
    let holder = Holder(c);
    let (tx, lines) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    wait_line(&lines, "locked ", "the lock holder ended before locking");
    (holder, lines)
}

/// Waits (bounded) for a line containing `want`.
fn wait_line(lines: &Lines, want: &str, ended: &str) {
    let deadline = Instant::now() + HOLDER_DEADLINE;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) if line.contains(want) => return,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                panic!("the lock holder did not say {want:?} in time")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("{ended}"),
        }
    }
}

/// Waits for the holder to say it released the lock, then (bounded) for
/// it to exit.
fn release(mut holder: Holder, lines: &Lines) {
    drop(holder.0.stdin.take());
    wait_line(lines, "released", "the lock holder ended without releasing");
    let deadline = Instant::now() + HOLDER_DEADLINE;
    let status = loop {
        if let Some(s) = holder.0.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < deadline, "the lock holder did not exit in time");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
}

#[test]
fn s2_busy_while_a_live_process_holds_the_lock() {
    let s = scratch("busy");
    drop(Store::open(&s, Access::Shared).unwrap());
    let (holder, out) = hold_lock(&s);
    let pid = holder.0.id();
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), format!("{pid}\n"));
    match Store::open(&s, Access::Exclusive) {
        Err(StoreError::Busy { pid: p, lock }) => {
            assert_eq!(p, pid);
            assert_eq!(lock, s.join("lock"));
        }
        other => panic!("expected Busy, got {other:?}"),
    }
    // The failed open did not disturb the lock, and Shared still works.
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), format!("{pid}\n"));
    let shared = Store::open(&s, Access::Shared).unwrap();
    shared.publish_plan(&plan("x").to_bytes()).unwrap();
    drop(shared);
    assert!(s.join("lock").exists());
    // Dropping the holder's Store removes the lock (another process's Drop).
    release(holder, &out);
    assert!(!s.join("lock").exists());
    drop(Store::open(&s, Access::Exclusive).unwrap());
}

#[test]
fn s2_busy_within_one_process() {
    let s = scratch("busy-self");
    let first = Store::open(&s, Access::Exclusive).unwrap();
    match Store::open(&s, Access::Exclusive) {
        Err(StoreError::Busy { pid, .. }) => assert_eq!(pid, std::process::id()),
        other => panic!("expected Busy, got {other:?}"),
    }
    assert!(s.join("lock").exists(), "the refused open must not remove the holder's lock");
    drop(first);
    assert!(!s.join("lock").exists());
}

#[test]
fn s2_dead_lock_taken_over_and_stale_tmp_removed() {
    let s = scratch("stale");
    drop(Store::open(&s, Access::Shared).unwrap());
    let dead = dead_pid();
    let me = std::process::id();
    fs::write(s.join("lock"), format!("{dead}\n")).unwrap();
    let tmp = s.join("tmp");
    fs::write(tmp.join(format!("{dead}.0.tmp")), "partial").unwrap();
    fs::write(tmp.join(format!("{dead}.17.tmp")), "partial").unwrap();
    // A dead process's scratch directory, with contents (§2.1).
    fs::create_dir_all(tmp.join(format!("{dead}.d/sub"))).unwrap();
    fs::write(tmp.join(format!("{dead}.d/sub/f")), "x").unwrap();
    // Entries of live processes and names not of the store's form stay.
    fs::write(tmp.join(format!("{me}.999999.tmp")), "live").unwrap();
    fs::write(tmp.join("foreign"), "?").unwrap();
    let store = Store::open(&s, Access::Exclusive).unwrap();
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), format!("{me}\n"));
    let mut expect = vec![format!("{me}.999999.tmp"), "foreign".to_owned()];
    expect.sort();
    assert_eq!(names(&tmp), expect);
    drop(store);
    assert!(!s.join("lock").exists());
    // The store's Drop only removes its own files.
    assert_eq!(names(&tmp), expect);
}

#[test]
fn s2_unreadable_lock_is_an_error_not_a_takeover() {
    let s = scratch("badlock");
    drop(Store::open(&s, Access::Shared).unwrap());
    fs::write(s.join("lock"), "not a pid").unwrap();
    assert!(matches!(Store::open(&s, Access::Exclusive), Err(StoreError::Io { .. })));
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), "not a pid");
}

#[test]
fn s2_shared_neither_locks_nor_cleans() {
    let s = scratch("shared");
    drop(Store::open(&s, Access::Shared).unwrap());
    let dead = dead_pid();
    fs::write(s.join("lock"), format!("{dead}\n")).unwrap();
    fs::write(s.join("tmp").join(format!("{dead}.0.tmp")), "partial").unwrap();
    fs::create_dir_all(s.join("tmp").join(format!("{dead}.d"))).unwrap();
    let store = Store::open(&s, Access::Shared).unwrap();
    store.publish_plan(&plan("x").to_bytes()).unwrap();
    drop(store);
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), format!("{dead}\n"));
    assert_eq!(names(&s.join("tmp")), [format!("{dead}.0.tmp"), format!("{dead}.d")]);
}

/// §2 "Cleaning", WP-018: `remove_entries_of(pid)` removes exactly the
/// `tmp/` entries named `<pid>.…`, directories with their contents
/// included, in either access mode and without asking `/proc`; other
/// processes' entries, names that only look alike, and this process's own
/// entries stay. A store with no `tmp/` is not an error.
#[test]
fn s2_remove_entries_of_one_process() {
    let s = scratch("remove-of");
    let store = Store::open(&s, Access::Shared).unwrap();
    let tmp = s.join("tmp");
    let me = std::process::id();
    // Alive or not does not matter: the caller vouches for it.
    let pid = me + 1;
    fs::write(tmp.join(format!("{pid}.0.tmp")), "partial").unwrap();
    fs::write(tmp.join(format!("{pid}.fake.tmp")), "loose").unwrap();
    fs::create_dir_all(tmp.join(format!("{pid}.d")).join("sub")).unwrap();
    fs::write(tmp.join(format!("{pid}.d")).join("sub").join("f"), "x").unwrap();
    let keep = [
        format!("{}.0.tmp", pid + 1),
        format!("0{pid}.0.tmp"),
        format!("{pid}0.0.tmp"),
        format!("{pid}"),
        format!("x{pid}.0.tmp"),
        format!("{me}.0.tmp"),
    ];
    for k in &keep {
        fs::write(tmp.join(k), "keep").unwrap();
    }
    let own = store.scratch_dir().unwrap();
    store.remove_entries_of(pid).unwrap();
    let mut want: Vec<String> = keep.to_vec();
    want.push(format!("{me}.d"));
    want.sort();
    assert_eq!(names(&tmp), want);
    // Its own id: nothing is removed.
    store.remove_entries_of(me).unwrap();
    assert!(own.exists());
    assert!(tmp.join(format!("{me}.0.tmp")).exists());
    // An exclusive store too, and again (nothing left: not an error).
    drop(store);
    fs::remove_file(tmp.join(format!("{me}.0.tmp"))).unwrap();
    let store = Store::open(&s, Access::Exclusive).unwrap();
    fs::create_dir_all(tmp.join(format!("{pid}.d"))).unwrap();
    store.remove_entries_of(pid).unwrap();
    store.remove_entries_of(pid).unwrap();
    assert!(!tmp.join(format!("{pid}.d")).exists());
    drop(store);
    fs::remove_dir_all(&tmp).unwrap();
    let store = Store::open(&s, Access::Shared).unwrap();
    store.remove_entries_of(pid).unwrap();
}

// ---------------------------------------------------------------------------
// Cleaning on Drop and on a signal (§2 "Cleaning", §7)

/// The store's in-flight tmp/ files cannot be observed from outside a
/// publish, so this checks the observable rule: after Drop, neither the
/// lock nor any tmp/ entry of this process made by that store remains.
#[test]
fn s2_drop_removes_lock_tmp_files_and_scratch() {
    let s = scratch("drop");
    let store = Store::open(&s, Access::Exclusive).unwrap();
    let scratch_dir = store.scratch_dir().unwrap();
    assert_eq!(scratch_dir, s.join("tmp").join(format!("{}.d", std::process::id())));
    assert!(scratch_dir.is_dir());
    // Idempotent.
    assert_eq!(store.scratch_dir().unwrap(), scratch_dir);
    fs::create_dir_all(scratch_dir.join("a/b")).unwrap();
    fs::write(scratch_dir.join("a/b/c"), "x").unwrap();
    store.publish_plan(&plan("x").to_bytes()).unwrap();
    drop(store);
    assert!(!s.join("lock").exists());
    assert!(names(&s.join("tmp")).is_empty());
    assert_eq!(names(&s.join("plans")).len(), 1, "published files stay");
}

#[test]
fn s7_cleanup_on_signal_removes_lock_and_scratch_and_closes() {
    let s = scratch("signal");
    let store = Store::open(&s, Access::Exclusive).unwrap();
    let dir = store.scratch_dir().unwrap();
    fs::write(dir.join("f"), "x").unwrap();
    let cleanup = cleanup_on_signal(&store);
    // It is 'static and Send: it can be handed to signals::install.
    let handle = std::thread::spawn(cleanup);
    handle.join().unwrap();
    assert!(!s.join("lock").exists());
    assert!(names(&s.join("tmp")).is_empty());
    // Afterwards the store creates no tmp/ files.
    assert!(store.publish_plan(&plan("y").to_bytes()).is_err());
    assert!(store.scratch_dir().is_err());
    assert!(names(&s.join("tmp")).is_empty());
    drop(store);
    assert!(names(&s.join("tmp")).is_empty());
}

#[test]
fn s2_lock_of_another_process_survives_drop() {
    // A store that held the lock does not remove a lock someone else has
    // since taken (here: rewritten by hand, as after a takeover).
    let s = scratch("foreign-lock");
    let store = Store::open(&s, Access::Exclusive).unwrap();
    fs::write(s.join("lock"), "1\n").unwrap();
    drop(store);
    assert_eq!(fs::read_to_string(s.join("lock")).unwrap(), "1\n");
}
