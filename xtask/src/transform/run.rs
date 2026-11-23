//! One transform test, run and judged (`spec/harness.md` §6.3): the work
//! directory, the database, every command line given to `slip` (built in
//! [`cmdline`] and nowhere else), reading a run's output, the comparison
//! with the golden files, `--bless`, and the order of the oracles.

use super::oracles;
use super::test_file::{Allow, Expected, Oracle, Test};
use super::tree::{self, Entry, Files};
use super::{Note, Outcome, Report};
use slip_model::Digest;
use slip_model::declared::Declared;
use slip_model::editset::{EditSet, SetState};
use slip_model::formats;
use slip_model::json::{self, Value};
use slip_model::overlay::Overlay;
use slip_model::result::{State, TransformResult};
use slip_verify::conclude;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The command lines the runner gives `slip`, in one place, so that the
/// command packages can be checked against them
/// (`spec/landed/harness/WP-077.md` §2).
pub mod cmdline {
    use super::{Allow, Ctx};
    use std::ffi::OsString;
    use std::path::Path;

    /// `slip <run> --db <work>/cdb.json --owned <tree>/<p>… --readonly
    /// <tree>/<p>… --store <store> --relative-to <tree> --write -j <jobs>
    /// --allow-<v>…` (§6.3 step 2).
    pub fn transform(ctx: &Ctx<'_>, run: &[String], store: &Path, jobs: u32) -> Vec<OsString> {
        let mut a: Vec<OsString> = run.iter().map(OsString::from).collect();
        a.push("--db".into());
        a.push(ctx.db.clone().into());
        for p in &ctx.test.owned {
            a.push("--owned".into());
            a.push(ctx.tree_path(p).into());
        }
        for p in &ctx.test.readonly {
            a.push("--readonly".into());
            a.push(ctx.tree_path(p).into());
        }
        a.push("--store".into());
        a.push(store.into());
        a.push("--relative-to".into());
        a.push(ctx.tree.clone().into());
        a.push("--write".into());
        a.push("-j".into());
        a.push(jobs.to_string().into());
        for allow in &ctx.test.allow {
            a.push(format!("--allow-{}", allow.name()).into());
        }
        a
    }

    /// `slip check --plan <plan id> --round <n> --edits <edit set>
    /// [--declared <declared id>] --store <store> -j 2 [--allow-unparsed]`
    /// (§6.4 `verify` and `mutation`; `spec/edits.md` §10.2,
    /// `spec/exec.md` §8).
    pub fn check(
        ctx: &Ctx<'_>,
        store: &Path,
        plan: &str,
        round: &str,
        edits: &OsString,
        declared: Option<&str>,
    ) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec![
            "check".into(),
            "--plan".into(),
            plan.into(),
            "--round".into(),
            round.into(),
            "--edits".into(),
            edits.clone(),
        ];
        if let Some(d) = declared {
            a.push("--declared".into());
            a.push(d.into());
        }
        a.push("--store".into());
        a.push(store.into());
        a.push("-j".into());
        a.push("2".into());
        if ctx.test.allows(Allow::Unparsed) {
            a.push("--allow-unparsed".into());
        }
        a
    }

    /// The inverse of a rename (§6.4 `back`): `rename --from a::f --to g`
    /// becomes `rename --from a::g --to f`, every other word kept. `None`
    /// for a run that is not a rename with one `--from <name>` and one
    /// `--to <identifier>`.
    pub fn inverse(run: &[String]) -> Option<Vec<String>> {
        if run.first().map(String::as_str) != Some("rename") {
            return None;
        }
        let at = |opt: &str| -> Option<usize> {
            let mut found = run.iter().enumerate().filter(|(_, w)| *w == opt).map(|(i, _)| i);
            let i = found.next()?;
            if found.next().is_some() || run.iter().any(|w| w.starts_with(&format!("{opt}="))) {
                return None;
            }
            (i + 1 < run.len()).then_some(i + 1)
        };
        let (from, to) = (at("--from")?, at("--to")?);
        let old = &run[from];
        let new = &run[to];
        if new.contains("::") || new.is_empty() {
            return None;
        }
        let (inverse_from, inverse_to) = match old.rsplit_once("::") {
            Some((prefix, last)) if !last.is_empty() => {
                (format!("{prefix}::{new}"), last.to_owned())
            }
            Some(_) => return None,
            None => (new.clone(), old.clone()),
        };
        let mut out = run.to_vec();
        out[from] = inverse_from;
        out[to] = inverse_to;
        Some(out)
    }
}

/// A test being run: its paths.
#[derive(Debug)]
pub struct Ctx<'a> {
    pub test: &'a Test,
    pub slip: &'a Path,
    /// The work directory, canonical.
    pub work: PathBuf,
    /// `<work>/tree`: the copy of `pre/` every `slip` runs in.
    pub tree: PathBuf,
    /// `<work>/cdb.json`.
    pub db: PathBuf,
    /// `<work>/store`.
    pub store: PathBuf,
    /// The commands of the database: (directory, argv).
    pub commands: Vec<(PathBuf, Vec<String>)>,
    pub pre: Files,
    /// The test's own directory as it was before the first run (and after
    /// `--bless`): every run must leave it so.
    pub test_snapshot: Files,
}

/// The entries a work directory may hold (`spec/landed/harness/WP-077.md`):
/// any other was written by a run outside its tree.
pub const WORK_ENTRIES: [&str; 6] = ["back", "cdb.json", "clang", "determinism", "store", "tree"];

impl Ctx<'_> {
    /// A path of `owned`/`readonly` in the tree; `.` is the tree.
    pub fn tree_path(&self, p: &str) -> PathBuf {
        if p == "." { self.tree.clone() } else { self.tree.join(p) }
    }

    /// Removes the tree and copies `pre/` to it again.
    pub fn restore_tree(&self) -> Result<(), String> {
        tree::remove(&self.tree).map_err(|e| format!("{}: {e}", self.tree.display()))?;
        tree::copy_tree(&self.test.dir.join("pre"), &self.tree)
            .map_err(|e| format!("copying pre/ to {}: {e}", self.tree.display()))
    }

    pub fn read_tree(&self) -> Result<Files, String> {
        tree::read_tree(&self.tree).map_err(|e| format!("the tree: {e}"))
    }

    /// The test's timeout.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.test.timeout)
    }

    /// Runs `slip` with `args` in the tree, bounded by the test's timeout;
    /// then checks that it wrote nothing outside the tree, its store and
    /// the oracles' directories ([`WORK_ENTRIES`]) and left the test's
    /// directory as it was.
    pub fn slip(&self, args: &[OsString]) -> Result<Ran, Outcome> {
        let shown = args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
        let mut cmd = Command::new(self.slip);
        cmd.args(args).current_dir(&self.tree);
        let out = match run_bounded(cmd, self.timeout()) {
            Ok(o) => o,
            Err(Bounded::Spawn(e)) => {
                return Err(Outcome::Error(format!("cannot run `{}`: {e}", self.slip.display())));
            }
            Err(Bounded::TimedOut) => {
                return Err(Outcome::Killed(format!(
                    "`slip {shown}` did not end within {} s; its process group was killed",
                    self.test.timeout
                )));
            }
        };
        if out.status.code().is_none() {
            use std::os::unix::process::ExitStatusExt as _;
            let signal = out.status.signal().unwrap_or(0);
            return Err(Outcome::Killed(format!("`slip {shown}` killed by signal {signal}")));
        }
        self.check_surroundings(&shown)?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        Ok(Ran {
            status: out.status.code().unwrap_or(-1),
            lines: Lines::read(&stdout),
            stdout,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    /// After a run: the work directory holds only [`WORK_ENTRIES`] (and
    /// `back/`, `determinism/` only `store`), and the test's directory is
    /// [`Ctx::test_snapshot`].
    fn check_surroundings(&self, shown: &str) -> Result<(), Outcome> {
        let names = |dir: &Path| -> Result<Vec<String>, Outcome> {
            let mut v = Vec::new();
            for e in std::fs::read_dir(dir).map_err(|e| error(format!("{}: {e}", dir.display())))? {
                let e = e.map_err(|e| error(format!("{}: {e}", dir.display())))?;
                v.push(e.file_name().to_string_lossy().into_owned());
            }
            v.sort();
            Ok(v)
        };
        for n in names(&self.work)? {
            let stray = if WORK_ENTRIES.contains(&n.as_str()) {
                if n == "back" || n == "determinism" {
                    names(&self.work.join(&n))?
                        .into_iter()
                        .find(|m| m != "store")
                        .map(|m| format!("{n}/{m}"))
                } else {
                    None
                }
            } else {
                Some(n)
            };
            if let Some(stray) = stray {
                return Err(Outcome::Fail(format!(
                    "`slip {shown}` wrote outside its tree: {}",
                    self.work.join(stray).display()
                )));
            }
        }
        let now = tree::read_tree(&self.test.dir).map_err(|e| error(format!("the test: {e}")))?;
        if let Some(d) = tree::first_difference(&self.test_snapshot, &now) {
            return Err(Outcome::Fail(format!("`slip {shown}` changed the test's directory: {d}")));
        }
        Ok(())
    }
}

/// A finished child of [`run_bounded`].
#[derive(Debug)]
pub struct Finished {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why [`run_bounded`] has no [`Finished`].
#[derive(Debug)]
pub enum Bounded {
    Spawn(std::io::Error),
    /// The child, or a process that kept its standard output or error open,
    /// was still running at the deadline; the child's process group was
    /// killed.
    TimedOut,
}

/// Runs `cmd` with standard input closed and its output captured, in a
/// process group of its own. It is done when the child has ended and both
/// pipes are closed (a grandchild that holds one open holds the run); at
/// `timeout` the group is killed (`kill -KILL -- -<pgid>`: the standard
/// library has no call for a group, and `xtask` has no `unsafe`).
pub fn run_bounded(mut cmd: Command, timeout: Duration) -> Result<Finished, Bounded> {
    use std::io::Read as _;
    use std::os::unix::process::CommandExt as _;
    cmd.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(Bounded::Spawn)?;
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel::<(usize, Vec<u8>)>();
    let pipes: [Option<Box<dyn std::io::Read + Send>>; 2] = [
        child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    ];
    for (k, pipe) in pipes.into_iter().enumerate() {
        let tx = tx.clone();
        if let Some(mut pipe) = pipe {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                let _ = tx.send((k, buf));
            });
        }
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut out: [Option<Vec<u8>>; 2] = [None, None];
    loop {
        if status.is_none() {
            status = child.try_wait().map_err(Bounded::Spawn)?;
        }
        if let Some(s) = status
            && out.iter().all(Option::is_some)
        {
            let [stdout, stderr] = out;
            return Ok(Finished {
                status: s,
                stdout: stdout.unwrap_or_default(),
                stderr: stderr.unwrap_or_default(),
            });
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = Command::new(kill_program())
                .args(["-KILL", "--", &format!("-{pid}")])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if status.is_none() {
                let _ = child.wait();
            }
            return Err(Bounded::TimedOut);
        }
        let step = (deadline - now).min(Duration::from_millis(10));
        match rx.recv_timeout(step) {
            Ok((k, bytes)) => out[k] = Some(bytes),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            // Both pipes are read: wait for the child itself.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(step),
        }
    }
}

/// The `kill` program: `/bin/kill` or `/usr/bin/kill` when there (the run
/// may have a `PATH` without it; a shell's built-in `kill` may not take a
/// process group), else `kill` on `PATH`.
pub fn kill_program() -> &'static str {
    ["/bin/kill", "/usr/bin/kill"].into_iter().find(|p| Path::new(p).is_file()).unwrap_or("kill")
}

/// One finished invocation of `slip`.
#[derive(Debug, Clone)]
pub struct Ran {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub lines: Lines,
}

/// The ids and the state a run printed (`spec/edits.md` §10.5, the plan
/// line of `spec/exec.md` §8.1). Each is the first such line, except
/// `state`, which is the last (the result's, by [`check_output`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines {
    pub plan: Option<String>,
    pub transform: Option<String>,
    pub round: Option<String>,
    pub edits: Option<String>,
    pub declared: Option<String>,
    pub overlay: Option<String>,
    pub result: Option<String>,
    pub state: Option<String>,
}

impl Lines {
    pub fn read(stdout: &str) -> Lines {
        let mut l = Lines::default();
        for line in stdout.lines() {
            let Some((word, rest)) = line.split_once(' ') else { continue };
            // `declared <id>: …` and `overlay <id>: …` end their id at `:`.
            let value = rest.split_once(':').map_or(rest, |(v, _)| v);
            let slot = match word {
                "plan" => &mut l.plan,
                "transform" => &mut l.transform,
                "round" => &mut l.round,
                "edits" => &mut l.edits,
                "declared" => &mut l.declared,
                "overlay" => &mut l.overlay,
                "result" => &mut l.result,
                "state" => {
                    l.state = Some(rest.to_owned());
                    continue;
                }
                _ => continue,
            };
            let is_word = !value.is_empty() && !value.contains(' ');
            if slot.is_none() && is_word {
                *slot = Some(value.to_owned());
            }
        }
        l
    }

    /// The ids the `determinism` oracle compares, by name.
    pub fn ids(&self) -> [(&'static str, &Option<String>); 5] {
        [
            ("transform", &self.transform),
            ("edit set", &self.edits),
            ("declared", &self.declared),
            ("overlay", &self.overlay),
            ("result", &self.result),
        ]
    }
}

/// The names of the states of `spec/edits.md` §8.5, as a `state` line
/// after `result` prints them.
pub const STATES: [&str; 5] =
    ["verified", "verified_with_residue", "not_verified", "incomplete", "refused"];

/// The structure of a run's output (`spec/edits.md` §10.5), checked before
/// it is compared or blessed. With `result` (a run that ends in a state):
/// one `result <id>` line, followed at once by one `state <State>` with a
/// state of §8.5 exactly; the only other `state` line is the edit set's,
/// `state accepted` or `state refused` at once after its `edits <id>`; the
/// result's `state` line is the last. Without (`usage`, `input`): no
/// `result` line and no `state` line but the edit set's. In both, each of
/// `plan`, `transform`, `round`, `edits`, `declared`, `overlay` and
/// `result` at most once.
pub fn check_output(stdout: &str, result: bool) -> Result<(), String> {
    let lines: Vec<&str> = stdout.lines().collect();
    for word in ["plan", "transform", "round", "edits", "declared", "overlay", "result"] {
        let n = lines.iter().filter(|l| l.split(' ').next() == Some(word)).count();
        if n > 1 {
            return Err(format!("the output has {n} `{word}` lines"));
        }
    }
    let has_result = lines.iter().any(|l| l.split(' ').next() == Some("result"));
    if result && !has_result {
        // Nothing to check the state lines against: the missing result is
        // the state's difference (`state_difference`, with standard error).
        return Ok(());
    }
    let (mut result_state, mut last_state) = (None, None);
    for (i, line) in lines.iter().enumerate() {
        let Some(value) = line.strip_prefix("state ") else {
            if *line == "state" {
                return Err(format!("output line {}: `state` without a value", i + 1));
            }
            continue;
        };
        last_state = Some(i);
        let after = |w: &str| i > 0 && lines[i - 1].split(' ').next() == Some(w);
        if after("edits") {
            if value != "accepted" && value != "refused" {
                return Err(format!(
                    "output line {}: the edit set's state `{value}` is not accepted or refused",
                    i + 1
                ));
            }
        } else if after("result") && result {
            if !STATES.contains(&value) {
                return Err(format!(
                    "output line {}: `{value}` is not a state of spec/edits.md §8.5",
                    i + 1
                ));
            }
            result_state = Some(i);
        } else {
            return Err(format!(
                "output line {}: a `state` line that is neither the edit set's nor the result's",
                i + 1
            ));
        }
    }
    if !result {
        if has_result {
            return Err("the output has a `result` line, and the run ends without a result".into());
        }
        return Ok(());
    }
    if has_result && result_state.is_none() {
        return Err("the `result` line is not followed by the result's `state` line".into());
    }
    if has_result && result_state != last_state {
        return Err("the result's `state` line is not the last `state` line".into());
    }
    Ok(())
}

/// §6.3 step 3: every 64-digit id (a maximal run of exactly 64 lower-case
/// hexadecimal digits) becomes `<id>`, the store's path `@STORE@` and the
/// tree's path `@ROOT@`.
pub fn normalise(stdout: &str, tree: &Path, store: &Path) -> String {
    let mut s = stdout.to_owned();
    let (tree, store) = (tree.to_string_lossy(), store.to_string_lossy());
    // The longer first, so that neither is replaced inside the other.
    let mut paths = [(store.as_ref(), "@STORE@"), (tree.as_ref(), "@ROOT@")];
    paths.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    for (p, with) in paths {
        if !p.is_empty() {
            s = s.replace(p, with);
        }
    }
    replace_ids(&s)
}

fn replace_ids(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if hex(b[i]) {
            let start = i;
            while i < b.len() && hex(b[i]) {
                i += 1;
            }
            if i - start == 64 {
                out.push_str("<id>");
            } else {
                out.push_str(&s[start..i]);
            }
        } else {
            let start = i;
            while i < b.len() && !hex(b[i]) {
                i += 1;
            }
            out.push_str(&s[start..i]);
        }
    }
    out
}

/// The first difference of `got` from `expected`, line by line, as
/// ``output line <n>: expected `…`, got `…` `` (`the end` past the last).
pub fn first_line_difference(expected: &str, got: &str) -> Option<String> {
    if expected == got {
        return None;
    }
    let lines = |s: &str| -> Vec<String> {
        let body = s.strip_suffix('\n').unwrap_or(s);
        if s.is_empty() { Vec::new() } else { body.split('\n').map(str::to_owned).collect() }
    };
    let (e, g) = (lines(expected), lines(got));
    let shown = |l: Option<&String>| l.map_or("the end".to_owned(), |l| format!("`{l}`"));
    for i in 0..e.len().max(g.len()) {
        if e.get(i) != g.get(i) {
            return Some(format!(
                "output line {}: expected {}, got {}",
                i + 1,
                shown(e.get(i)),
                shown(g.get(i))
            ));
        }
    }
    // The same lines: one of the two ends without a newline.
    Some(format!("output line {}: the newline at its end differs", e.len()))
}

// ---------------------------------------------------------------- the store

/// The records of a run in its store (`spec/edits.md` §10.4), read and
/// checked against what the run printed.
#[derive(Debug, Clone)]
pub struct Record {
    pub id: Digest,
    pub bytes: Vec<u8>,
    pub result: TransformResult,
    pub set_id: Digest,
    pub set: EditSet,
    pub declared: Option<Declared>,
    pub overlay: Option<Overlay>,
}

/// The path of an object in a store (`spec/edits.md` §10.4, `spec/exec.md`
/// §2): `<store>/<dir>/<id>.<ext>`.
pub fn object_path(store: &Path, dir: &str, id: &Digest, ext: &str) -> PathBuf {
    store.join(dir).join(format!("{id}.{ext}"))
}

/// Reads the object `<store>/<dir>/<id>.<ext>`, which must exist, be named
/// by its content id and read as `T` (with every rule of its format).
pub fn read_object<T>(
    store: &Path,
    what: &str,
    (dir, ext): (&str, &str),
    id: &Digest,
    decode: impl Fn(&[u8]) -> Result<T, slip_model::wire::Error>,
) -> Result<(Vec<u8>, T), String> {
    let path = object_path(store, dir, id, ext);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("the store has no {what} {id}"));
        }
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match formats::content_id(&bytes) {
        Ok(got) if got == *id => {}
        Ok(got) => return Err(format!("the {what} {id} in the store has the content id {got}")),
        Err(e) => return Err(format!("the {what} {id}: {e}")),
    }
    let value = decode(&bytes).map_err(|e| format!("the {what} {id}: {e}"))?;
    Ok((bytes, value))
}

/// An id printed by a run.
pub fn digest(what: &str, id: &str) -> Result<Digest, String> {
    Digest::from_hex(id).ok_or(format!("the {what} id `{id}` is not 64 hexadecimal digits"))
}

/// Reads the result `id` of `store`.
pub fn read_result(store: &Path, id: &str) -> Result<(Vec<u8>, TransformResult), String> {
    let id = digest("result", id)?;
    read_object(store, "result", ("results", "rslt"), &id, TransformResult::from_bytes)
}

/// The run's records (F1 of the review, `spec/landed/harness/WP-077.md`):
/// the result its `result` line names, whose state is the printed state,
/// whose edit set, declared changes and overlay are the printed ids and
/// are in the store, valid; whose plan, round and transform are the
/// printed ones and the edit set's; and no `write.journal` is left.
pub fn read_record(ctx: &Ctx<'_>, ran: &Ran) -> Result<Record, String> {
    let store = &ctx.store;
    if store.join("write.journal").exists() {
        return Err("the store holds write.journal: a write was left unfinished".into());
    }
    let l = &ran.lines;
    let Some(rid) = &l.result else { return Err("the run printed no result line".into()) };
    let (bytes, result) = read_result(store, rid)?;
    let id = digest("result", rid)?;
    let printed = l.state.as_deref().unwrap_or("none");
    if printed != result.state.name() {
        return Err(format!(
            "the run printed state {printed}, its result record says {}",
            result.state.name()
        ));
    }
    let shown = |d: Option<Digest>| d.map_or("none".to_owned(), |d| d.to_string());
    let lines: [(&str, &Option<String>, Option<Digest>); 3] = [
        ("edits", &l.edits, Some(result.edits)),
        ("declared", &l.declared, result.declared),
        ("overlay", &l.overlay, result.overlay),
    ];
    for (name, printed, recorded) in lines {
        if printed.as_deref() != recorded.map(|d| d.to_string()).as_deref() {
            return Err(format!(
                "the run printed {name} {}, its result record says {}",
                printed.as_deref().unwrap_or("none"),
                shown(recorded)
            ));
        }
    }
    for (name, printed, recorded) in [
        ("plan", &l.plan, result.plan.to_string()),
        ("transform", &l.transform, result.transform.to_string()),
        ("round", &l.round, result.round.to_string()),
    ] {
        if let Some(p) = printed
            && *p != recorded
        {
            return Err(format!("the run printed {name} {p}, its result record says {recorded}"));
        }
    }
    let (_, set) =
        read_object(store, "edit set", ("edits", "edts"), &result.edits, EditSet::from_bytes)?;
    if (set.plan, set.round, set.transform) != (result.plan, result.round, result.transform) {
        return Err(format!(
            "the result {rid} and its edit set {} differ in plan, round or transform",
            result.edits
        ));
    }
    if matches!(result.state, State::Verified | State::VerifiedWithResidue)
        && set.state != SetState::Accepted
    {
        return Err(format!(
            "the result is {} and its edit set {} is not accepted",
            result.state.name(),
            result.edits
        ));
    }
    let declared = match result.declared {
        Some(d) => Some(
            read_object(store, "declared changes", ("declared", "decl"), &d, Declared::from_bytes)?
                .1,
        ),
        None => None,
    };
    let overlay = match result.overlay {
        Some(o) => {
            let (_, ov) =
                read_object(store, "overlay", ("overlays", "ovly"), &o, Overlay::from_bytes)?;
            if ov.edits != result.edits {
                return Err(format!(
                    "the overlay {o} is of the edit set {}, the result's is {}",
                    ov.edits, result.edits
                ));
            }
            Some(ov)
        }
        None => None,
    };
    if result.allow_unparsed != ctx.test.allows(Allow::Unparsed) {
        return Err(format!(
            "the result says allow_unparsed {}, and the run was{} given --allow-unparsed",
            result.allow_unparsed,
            if ctx.test.allows(Allow::Unparsed) { "" } else { " not" }
        ));
    }
    Ok(Record { id, bytes, result, set_id: set_digest(&set), set, declared, overlay })
}

fn set_digest(set: &EditSet) -> Digest {
    formats::content_id(&set.to_bytes()).unwrap_or(Digest([0; 32]))
}

/// Whether the run's result permits `--write` under the test's `allow`
/// lines: `slip_verify::conclude::permits` over its records (never the
/// expected state's name).
pub fn permitted(test: &Test, record: Option<&Record>) -> bool {
    let allow = conclude::Allow {
        residue: test.allows(Allow::Residue),
        errors: test.allows(Allow::Errors),
        partial: test.allows(Allow::Partial),
    };
    record.is_some_and(|r| conclude::permits(&r.result, &r.set, allow))
}

// ---------------------------------------------------------------- the test

/// Runs and judges `test` (§6.3, §6.4) in `work`; see [`super::run_one`].
pub fn run_test(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
) -> Report {
    let mut report = Report { outcome: Outcome::Pass, clang_skipped: false, notes: Vec::new() };
    for (oracle, reason) in &test.oracle_off {
        report.notes.push(Note { oracle: *oracle, off: true, reason: reason.clone() });
    }
    let judged = judge(test, slip, work, clang, bless, &mut report);
    report.outcome = match (judged, &test.xfail) {
        (Ok(()), None) => Outcome::Pass,
        (Ok(()), Some(_)) => Outcome::XPass,
        (Err(Outcome::Fail(_)), Some(_)) => Outcome::XFail,
        (Err(o), _) => o,
    };
    report
}

fn error<E: std::fmt::Display>(e: E) -> Outcome {
    Outcome::Error(e.to_string())
}

/// `Ok` for a pass; `Err` with the outcome otherwise (before `xfail`).
fn judge(
    test: &Test,
    slip: &Path,
    work: &Path,
    clang: Option<&Path>,
    bless: bool,
    report: &mut Report,
) -> Result<(), Outcome> {
    tree::check_work_dir(work, &test.area, &test.name, &test.dir).map_err(Outcome::Error)?;
    if !test.expect && !bless {
        return Err(Outcome::Error("no expect.txt (`--bless` writes it)".into()));
    }
    let pre_dir = test.dir.join("pre");
    let pre = tree::read_plain_tree(&pre_dir).map_err(|e| Outcome::Error(format!("pre/: {e}")))?;
    let post = if std::fs::symlink_metadata(test.dir.join("post")).is_ok() {
        tree::read_plain_tree(&test.dir.join("post"))
            .map_err(|e| Outcome::Error(format!("post/: {e}")))?
    } else {
        Files::new()
    };
    let test_snapshot =
        tree::read_tree(&test.dir).map_err(|e| Outcome::Error(format!("the test: {e}")))?;
    let work = tree::fresh_dir(work).map_err(|e| error(format!("{}: {e}", work.display())))?;
    let tree_dir = work.join("tree");
    let mut ctx = Ctx {
        test,
        slip,
        tree: tree_dir.clone(),
        db: work.join("cdb.json"),
        store: work.join("store"),
        work,
        commands: Vec::new(),
        pre: pre.clone(),
        test_snapshot,
    };
    tree::copy_tree(&pre_dir, &ctx.tree).map_err(|e| error(format!("copying pre/: {e}")))?;
    ctx.commands = write_database(&ctx)?;

    // The clang oracle's pre-image, while the tree is still pre/ (§6.4),
    // for a test whose run may write.
    if test.oracle_on(Oracle::Clang)
        && test.may_write()
        && let Some(clang) = clang
        && let Err(e) = oracles::clang(&ctx, clang, None)
    {
        return Err(Outcome::Error(format!("the pre-image is not accepted by clang: {e}")));
    }

    // §6.3 step 2: the run.
    let args = cmdline::transform(&ctx, &test.run, &ctx.store, 2);
    let ran = ctx.slip(&args)?;
    let got = ctx.read_tree().map_err(Outcome::Error)?;

    // The output's structure, before anything is compared or blessed.
    check_output(&ran.stdout, test.state.is_state())
        .map_err(|e| Outcome::Fail(format!("output: {e}")))?;
    // The run's records.
    // (A run with no result line is the state's difference already.)
    let record = if test.state.is_state() && ran.lines.result.is_some() {
        Some(read_record(&ctx, &ran))
    } else {
        None
    };
    let (record, record_error) = match record {
        Some(Ok(r)) => (Some(r), None),
        Some(Err(e)) => (None, Some(format!("store: {e}"))),
        None => (None, None),
    };
    let permits = permitted(test, record.as_ref());

    // Step 3: the goldens.
    if bless {
        bless_goldens(&ctx, &ran, &got, &post, record_error.as_deref(), permits)?;
        ctx.test_snapshot =
            tree::read_tree(&test.dir).map_err(|e| Outcome::Error(format!("the test: {e}")))?;
    } else {
        let expect = std::fs::read(test.dir.join("expect.txt"))
            .map_err(|e| error(format!("expect.txt: {e}")))?;
        let expect = String::from_utf8_lossy(&expect);
        let mut differences = compare(&ctx, &ran, &expect, &tree::overlay(&pre, &post), &got);
        differences.splice(0..0, record_error);
        if let Some(d) = write_difference(&ctx, record.as_ref(), permits, &got) {
            differences.push(d);
        }
        if !differences.is_empty() {
            return Err(Outcome::Fail(differences.join("; ")));
        }
    }

    // Step 4: the oracles.
    let failures = oracles::run_all(&ctx, &ran, record.as_ref(), &got, clang, report)?;
    if failures.is_empty() { Ok(()) } else { Err(Outcome::Fail(failures.join("; "))) }
}

/// A write the result does not permit: the tree differs from `pre/` and
/// [`permitted`] is false.
fn write_difference(
    ctx: &Ctx<'_>,
    record: Option<&Record>,
    permits: bool,
    got: &Files,
) -> Option<String> {
    let written = tree::first_difference(&ctx.pre, got)?;
    if permits {
        return None;
    }
    let state = record.map_or("none", |r| r.result.state.name());
    Some(format!(
        "the tree was written, and the result (state {state}) does not permit writing \
         (spec/edits.md §8.5): {written}"
    ))
}

/// Step 3's differences: the exit status, the state, the first line of
/// the output, the first entry of the tree.
fn compare(
    ctx: &Ctx<'_>,
    ran: &Ran,
    expect: &str,
    expected_tree: &Files,
    got: &Files,
) -> Vec<String> {
    let test = ctx.test;
    let mut d = Vec::new();
    if ran.status != test.exit {
        d.push(format!("exit status {}, expected {}", ran.status, test.exit));
    }
    if let Some(s) = state_difference(test.state, ran) {
        d.push(s);
    }
    let out = normalise(&ran.stdout, &ctx.tree, &ctx.store);
    if let Some(s) = first_line_difference(expect, &out) {
        d.push(s);
    }
    if let Some(s) = tree::first_difference(expected_tree, got) {
        d.push(format!("tree: {s}"));
    }
    d
}

/// The state's difference, if any: a state needs a `result` line (else the
/// standard error is shown) and the last `state` line.
pub fn state_difference(expected: Expected, ran: &Ran) -> Option<String> {
    if !expected.is_state() {
        return None;
    }
    if ran.lines.result.is_none() {
        return Some(format!(
            "the run printed no result line; standard error:\n{}",
            ran.stderr.trim_end()
        ));
    }
    let got = ran.lines.state.as_deref().unwrap_or("none");
    (got != expected.name()).then(|| format!("state {got}, expected {}", expected.name()))
}

/// `--bless` (§6.3): only when the state and the exit status are as
/// `test.txt` says, the records are the output's, the result permits what
/// was written, and the written tree is one `post/` (or `post: none`) can
/// describe.
fn bless_goldens(
    ctx: &Ctx<'_>,
    ran: &Ran,
    got: &Files,
    old_post: &Files,
    record_error: Option<&str>,
    permits: bool,
) -> Result<(), Outcome> {
    let test = ctx.test;
    let mut refuse = Vec::new();
    if ran.status != test.exit {
        refuse.push(format!("exit status {}, expected {}", ran.status, test.exit));
    }
    if let Some(s) = state_difference(test.state, ran) {
        refuse.push(s);
    }
    if let Some(e) = record_error {
        refuse.push(e.to_owned());
    }
    // Only changed regular files can be described by post/.
    let mut shape = Vec::new();
    for (p, e) in got {
        match (ctx.pre.get(p), e) {
            (Some(Entry::File { .. }), Entry::File { .. }) | (Some(Entry::Dir), Entry::Dir) => {}
            (None, _) => shape.push(format!("{} {p} added", e.kind())),
            (Some(pe), _) => shape.push(format!("{} {p} is now a {}", pe.kind(), e.kind())),
        }
    }
    for (p, e) in &ctx.pre {
        if !got.contains_key(p) {
            shape.push(format!("{} {p} missing", e.kind()));
        }
    }
    if let Some(d) = shape.into_iter().next() {
        refuse.push(format!("post/ cannot hold the written tree: {d}"));
    }
    let written = got != &ctx.pre;
    if written && !permits {
        refuse.push("the result does not permit writing (spec/edits.md §8.5)".into());
    }
    if written && test.post_none {
        refuse.push("the test says `post: none`, and the run wrote".into());
    }
    if !written && test.may_write() && !test.post_none {
        refuse.push("the run wrote nothing: say `post: none` in test.txt".into());
    }
    if !refuse.is_empty() {
        return Err(Outcome::Fail(format!("cannot bless: {}", refuse.join("; "))));
    }
    let io = |p: &Path, e: std::io::Error| Outcome::Error(format!("bless: {}: {e}", p.display()));
    let expect_path = test.dir.join("expect.txt");
    let out = normalise(&ran.stdout, &ctx.tree, &ctx.store);
    if std::fs::read(&expect_path).ok().as_deref() != Some(out.as_bytes()) {
        std::fs::write(&expect_path, &out).map_err(|e| io(&expect_path, e))?;
        println!("bless: {}", expect_path.display());
    }
    let post_dir = test.dir.join("post");
    let new_post: Files = got
        .iter()
        .filter(|(k, v)| matches!(v, Entry::File { .. }) && ctx.pre.get(*k) != Some(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in &new_post {
        if old_post.get(k) != Some(v)
            && let Entry::File { mode, bytes } = v
        {
            let p = post_dir.join(k);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            }
            std::fs::write(&p, bytes).map_err(|e| io(&p, e))?;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(*mode))
                .map_err(|e| io(&p, e))?;
            println!("bless: {}", p.display());
        }
    }
    for k in old_post
        .iter()
        .filter(|(k, v)| matches!(v, Entry::File { .. }) && !new_post.contains_key(*k))
        .map(|(k, _)| k)
    {
        let p = post_dir.join(k);
        std::fs::remove_file(&p).map_err(|e| io(&p, e))?;
        println!("bless: {} (removed)", p.display());
        // Directories left empty go too, up to post/ itself.
        let mut dir = p.parent();
        while let Some(d) = dir {
            if !d.starts_with(&post_dir) || std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    Ok(())
}

/// Writes `<work>/cdb.json`: the test's `cdb.json` with `@ROOT@` replaced
/// by the tree's path, or the default commands of §6.2. Returns the
/// commands as (directory, argv), for the clang oracle.
fn write_database(ctx: &Ctx<'_>) -> Result<Vec<(PathBuf, Vec<String>)>, Outcome> {
    let tree = ctx.tree.to_str().ok_or_else(|| error("the tree's path is not UTF-8"))?;
    if tree.bytes().any(|b| b == b'"' || b == b'\\' || b < 0x20) {
        return Err(error(format!("the tree's path `{tree}` cannot be written in JSON")));
    }
    let text = if ctx.test.cdb {
        let raw = std::fs::read(ctx.test.dir.join("cdb.json"))
            .map_err(|e| error(format!("cdb.json: {e}")))?;
        let raw = String::from_utf8(raw).map_err(|_| error("cdb.json: not UTF-8"))?;
        raw.replace("@ROOT@", tree)
    } else {
        let entries: Vec<Value> = ctx
            .test
            .tu
            .iter()
            .map(|file| {
                let mut argv = vec!["kiln".to_owned(), format!("-std={}", ctx.test.std)];
                argv.extend(ctx.test.args.iter().cloned());
                argv.extend([
                    format!("-I{tree}"),
                    format!("-I{tree}/dep"),
                    "-c".into(),
                    file.clone(),
                ]);
                Value::Object(vec![
                    ("directory".into(), Value::Str(tree.to_owned())),
                    ("arguments".into(), Value::Array(argv.into_iter().map(Value::Str).collect())),
                    ("file".into(), Value::Str(file.clone())),
                ])
            })
            .collect();
        json::write_pretty(&Value::Array(entries))
            .map_err(|e| error(format!("writing the database: {e}")))?
    };
    let commands = read_commands(&text).map_err(|e| error(format!("cdb.json: {e}")))?;
    std::fs::write(&ctx.db, &text).map_err(|e| error(format!("{}: {e}", ctx.db.display())))?;
    Ok(commands)
}

/// The commands of a compilation database: each entry's `directory` and
/// its `arguments`, or its `command` split at white space with single and
/// double quotes and backslash escapes (a POSIX shell's words, without
/// expansions).
pub fn read_commands(text: &str) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
    let v = json::parse(text.as_bytes()).map_err(|e| e.to_string())?;
    let entries = v.as_array().ok_or("not an array")?;
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let dir =
            e.get("directory").and_then(Value::as_str).ok_or(format!("entry {i}: no directory"))?;
        let argv = if let Some(a) = e.get("arguments").and_then(Value::as_array) {
            a.iter()
                .map(|w| {
                    w.as_str()
                        .map(str::to_owned)
                        .ok_or(format!("entry {i}: an argument is not a string"))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(c) = e.get("command").and_then(Value::as_str) {
            split_command(c).ok_or(format!("entry {i}: an unterminated quote"))?
        } else {
            return Err(format!("entry {i}: no arguments and no command"));
        };
        if argv.is_empty() {
            return Err(format!("entry {i}: an empty command"));
        }
        out.push((PathBuf::from(dir), argv));
    }
    Ok(out)
}

fn split_command(c: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = c.chars();
    while let Some(ch) = chars.next() {
        match ch {
            ' ' | '\t' | '\n' => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\\' => {
                word.push(chars.next()?);
                in_word = true;
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        x => word.push(x),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => word.push(chars.next()?),
                        x => word.push(x),
                    }
                }
            }
            x => {
                word.push(x);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(word);
    }
    Some(out)
}

/// The store of an oracle's own run: `<work>/<sub>/store`, `<sub>` made.
pub fn oracle_store(ctx: &Ctx<'_>, sub: &str) -> Result<PathBuf, Outcome> {
    let dir = ctx.work.join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| error(format!("{}: {e}", dir.display())))?;
    Ok(dir.join("store"))
}
