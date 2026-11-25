//! `cargo xtask corpus rename` (`spec/harness.md` §6.5): a native rename
//! over a corpus, with `slip rename`, and the five checks of its outcome.
//!
//! ```text
//! cargo xtask corpus rename <corpus> --from <name> --to <identifier> [--kind <k>]
//!         [--owned <path>]… [--expect <state>] [--accept] [--build] [-j <n>] [--slip <path>]
//! ```
//!
//! The command never edits C++ itself: it hands names to `slip rename`,
//! applies the patch `slip` wrote with `patch`, and compares files byte for
//! byte. Everything it makes is under the corpus directory
//! `<root>/<corpus>/` ([`Paths`]):
//!
//! - `slip-store/`: the store of every run, so the plan is built once and
//!   found again (`slip rename --db` finds an existing plan as `parse`
//!   does);
//! - `rename/<target>.patch` and `rename/<target>.vfs.json`: the patch and
//!   the clang overlay file of the last run of a target, kept;
//! - `rename-copy/` and `rename-build/` (`--build`): a copy of the source
//!   directory (`src/`, the whole checkout; its `.git` is not copied) with
//!   the patch applied, and its configured and built tree. Both are removed
//!   by explicit path when the checks pass and kept when one fails.
//!
//! The result file is `bench/results/<corpus>.rename.<target>.json`
//! ([`result_json`]); `<target>` is `--from` with `::` written `.`
//! ([`target_name`]).

use crate::corpus::{self, Corpus, Ctx, Layout, build_tree, rows};
use crate::invariants::Failure;
use crate::perf::{self, Measured};
use crate::slip_run::{self, Ran};
use crate::transform::run::{Bounded, Lines, cmdline, read_object, read_result, run_bounded};
use slip_exec::jobserver::Pool;
use slip_exec::store::{Access, Store};
use slip_model::editset::EditSet;
use slip_model::json::{self, Value};
use slip_model::overlay::Overlay;
use slip_model::result::{Category, State, TransformResult};
use slip_model::transform_run::{StepName, TransformRun};
use slip_model::{Digest, Language, Plan, RunRecord};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const USAGE: &str = "\
usage: cargo xtask corpus rename <corpus> --from <name> --to <identifier> [--kind <k>]
               [--owned <path>]… [--expect <state>] [--accept] [--build] [-j <n>]
               [--slip <path>]";

/// The clang runs of `--accept` that take longer than this are a failure
/// of the check, not a wait without end.
pub const CLANG_TIMEOUT: Duration = Duration::from_hours(1);

/// At most this many lines of a failing check's differences are printed.
pub const LISTED: usize = 20;

/// The top-level directory of the source directory that is not copied and
/// not compared: the clone's git metadata (see [`copy_tree`]).
pub const NOT_COPIED: &str = ".git";

/// The members of the result file that are compared exactly with the file
/// already there (`spec/harness.md` §6.5): everything deterministic.
pub const EXACT: [&str; 6] = ["state", "counters", "tally", "agreement", "spellings", "residue"];

// ---------------------------------------------------------------------------
// Arguments

/// The options after `rename <corpus>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub from: String,
    pub to: String,
    pub kind: Option<String>,
    /// As given; made real paths under the source directory by the command.
    pub owned: Vec<PathBuf>,
    /// `None`: `verified` or `verified_with_residue`.
    pub expect: Option<State>,
    pub accept: bool,
    pub build: bool,
    pub jobs: Option<usize>,
    /// The `slip` executable (as `invariants` and `demo` take it).
    pub slip: Option<PathBuf>,
}

/// The state named `name` (`spec/edits.md` §8.5).
pub fn state_named(name: &str) -> Option<State> {
    State::ALL.into_iter().find(|s| s.name() == name)
}

/// The corpus and the options; `Err` is a usage error.
pub fn parse_args(args: &[String]) -> Result<(String, Options), String> {
    let (corpus, rest) = match args {
        [c, rest @ ..] if !c.starts_with('-') => (c.clone(), rest),
        _ => return Err("missing corpus".to_owned()),
    };
    let mut o = Options::default();
    let (mut from, mut to) = (None, None);
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("`{arg}` needs a value"));
        let twice = || format!("`{arg}` is given twice");
        match arg.as_str() {
            "--from" if from.is_none() => from = Some(value()?),
            "--to" if to.is_none() => to = Some(value()?),
            "--kind" if o.kind.is_none() => o.kind = Some(value()?),
            "--owned" => o.owned.push(PathBuf::from(value()?)),
            "--expect" if o.expect.is_none() => {
                let v = value()?;
                o.expect = Some(state_named(&v).ok_or_else(|| {
                    format!(
                        "`--expect {v}` is not a state ({})",
                        State::ALL.map(State::name).join(", ")
                    )
                })?);
            }
            "--accept" if !o.accept => o.accept = true,
            "--build" if !o.build => o.build = true,
            "-j" if o.jobs.is_none() => {
                let v = value()?;
                let n = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0 && v.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| format!("-j: `{v}` is not a positive integer"))?;
                o.jobs = Some(n);
            }
            "--slip" if o.slip.is_none() => o.slip = Some(PathBuf::from(value()?)),
            "--from" | "--to" | "--kind" | "--expect" | "--accept" | "--build" | "-j"
            | "--slip" => return Err(twice()),
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    o.from = from.ok_or("missing `--from <name>`")?;
    o.to = to.ok_or("missing `--to <identifier>`")?;
    Ok((corpus, o))
}

fn identifier(s: &str) -> bool {
    let mut b = s.bytes();
    b.next().is_some_and(|c| c == b'_' || c.is_ascii_alphabetic())
        && b.all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

/// `<target>` of the result file's name: `--from` without a leading `::`,
/// each `::` written `.`. `--from` must be identifiers joined by `::`
/// (the names a rename takes), so the file name is a plain name.
pub fn target_name(from: &str) -> Result<String, String> {
    let bare = from.strip_prefix("::").unwrap_or(from);
    let parts: Vec<&str> = bare.split("::").collect();
    if parts.iter().all(|p| identifier(p)) {
        Ok(parts.join("."))
    } else {
        Err(format!("`--from {from}` is not a qualified name (identifiers joined by `::`)"))
    }
}

// ---------------------------------------------------------------------------
// Paths

/// What the command makes under the corpus directory `dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `<dir>/slip-store`.
    pub store: PathBuf,
    /// `<dir>/rename`: the patches and overlay files.
    pub files: PathBuf,
    /// `<dir>/rename/<target>.patch`.
    pub patch: PathBuf,
    /// `<dir>/rename/<target>.vfs.json`.
    pub vfs: PathBuf,
    /// `<dir>/rename-copy`.
    pub copy: PathBuf,
    /// `<dir>/rename-build`.
    pub build: PathBuf,
    /// `<dir>/rename-build/slip-store`: the store of the inverse rename.
    pub back_store: PathBuf,
}

impl Paths {
    pub fn new(dir: &Path, target: &str) -> Paths {
        let files = dir.join("rename");
        let build = dir.join("rename-build");
        Paths {
            store: dir.join("slip-store"),
            patch: files.join(format!("{target}.patch")),
            vfs: files.join(format!("{target}.vfs.json")),
            files,
            copy: dir.join("rename-copy"),
            back_store: build.join("slip-store"),
            build,
        }
    }
}

/// `bench/results/<corpus>.rename.<target>.json` in the repository.
pub fn result_path(repo: &Path, corpus: &str, target: &str) -> PathBuf {
    repo.join("bench").join("results").join(format!("{corpus}.rename.{target}.json"))
}

/// The same path with `.new` appended: where a run's file goes when it does
/// not replace the one there.
pub fn new_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".new");
    PathBuf::from(s)
}

// ---------------------------------------------------------------------------
// Checks

/// How a check ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok(String),
    Fail(String),
    /// Not asked for (`--accept`, `--build`).
    Skipped(String),
    /// An earlier check failed.
    NotRun,
}

impl Outcome {
    pub fn word(&self) -> &'static str {
        match self {
            Outcome::Ok(_) => "ok",
            Outcome::Fail(_) => "FAIL",
            Outcome::Skipped(_) => "skipped",
            Outcome::NotRun => "not run",
        }
    }

    /// The name in the result file.
    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Ok(_) => "ok",
            Outcome::Fail(_) => "fail",
            Outcome::Skipped(_) => "skipped",
            Outcome::NotRun => "not_run",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Outcome::Ok(d) | Outcome::Fail(d) | Outcome::Skipped(d) => d,
            Outcome::NotRun => "an earlier check failed",
        }
    }
}

/// One of the five checks, in order: `state`, `determinism`, `accept`,
/// `build`, `back`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub outcome: Outcome,
    pub wall_ns: u64,
}

impl Check {
    /// `check <name>: <word> (<detail>)`.
    pub fn line(&self) -> String {
        format!("check {}: {} ({})", self.name, self.outcome.word(), self.outcome.detail())
    }
}

/// Check 1: the state is `expect`, or `verified` or
/// `verified_with_residue` without one.
pub fn check_state(state: State, expect: Option<State>) -> Outcome {
    let ok = match expect {
        Some(e) => state == e,
        None => matches!(state, State::Verified | State::VerifiedWithResidue),
    };
    let wanted = expect.map_or("verified or verified_with_residue", State::name);
    if ok {
        Outcome::Ok(state.name().to_owned())
    } else {
        Outcome::Fail(format!("state {}, expected {wanted}", state.name()))
    }
}

// ---------------------------------------------------------------------------
// The records of a run

/// What the command reads back from the store after `slip rename`: the
/// result, its edit set, the transform run record that names the result,
/// and the measurements of the phase run records it names.
#[derive(Debug, Clone)]
pub struct Records {
    pub result_id: Digest,
    pub result: TransformResult,
    pub set: EditSet,
    pub run: TransformRun,
    /// `(step name, measured)` for each step with a phase run record.
    pub phases: Vec<(&'static str, Measured)>,
}

fn is_run(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|e| e == "run")
}

fn names_in(dir: &Path) -> Result<Vec<String>, String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", dir.display()))?;
        if let Some(n) = e.file_name().to_str() {
            v.push(n.to_owned());
        }
    }
    v.sort();
    Ok(v)
}

/// Reads the records of the result `result_id` (as printed) from `store`.
pub fn read_records(store: &Path, result_id: &str) -> Result<Records, String> {
    let (_, result) = read_result(store, result_id)?;
    let id = crate::transform::run::digest("result", result_id)?;
    let (_, set) =
        read_object(store, "edit set", ("edits", "edts"), &result.edits, EditSet::from_bytes)?;
    let runs = store.join("runs").join(result.plan.to_string());
    let names = names_in(&runs)?;
    let mut found: Option<TransformRun> = None;
    for n in names.iter().filter(|n| n.starts_with("transform.") && is_run(n)) {
        let p = runs.join(n);
        let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let r = TransformRun::from_bytes(&bytes).map_err(|e| format!("{}: {e}", p.display()))?;
        if r.result == Some(id)
            && found.as_ref().is_none_or(|f| r.started_unix_ns > f.started_unix_ns)
        {
            found = Some(r);
        }
    }
    let run = found.ok_or_else(|| {
        format!("{}: no transform run record names the result {id}", runs.display())
    })?;
    let mut phases = Vec::new();
    for step in &run.steps {
        let Some(rid) = step.run else { continue };
        let prefix = match step.name {
            StepName::Map => "map.",
            StepName::Verify => "verify.",
            other => {
                return Err(format!(
                    "the transform run record's step `{}` names a run record",
                    other.name()
                ));
            }
        };
        let mut record = None;
        for n in names.iter().filter(|n| n.starts_with(prefix) && is_run(n)) {
            let p = runs.join(n);
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            if slip_model::formats::content_id(&bytes).ok() == Some(rid) {
                record = Some(
                    RunRecord::from_bytes(&bytes).map_err(|e| format!("{}: {e}", p.display()))?,
                );
                break;
            }
        }
        let record = record.ok_or_else(|| {
            format!("{}: no {} run record {rid}, which the transform run names", runs.display(), {
                step.name.name()
            })
        })?;
        phases.push((step.name.name(), perf::measured(&record)?));
    }
    Ok(Records { result_id: id, result, set, run, phases })
}

// ---------------------------------------------------------------------------
// The result file

fn int(n: u64) -> Value {
    Value::Int(i128::from(n))
}

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Object(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn id_or_null(d: Option<Digest>) -> Value {
    d.map_or(Value::Null, |d| Value::Str(d.to_string()))
}

/// What the result file says of the run besides its records.
#[derive(Debug, Clone)]
pub struct Header<'a> {
    pub corpus: &'a Corpus,
    pub opts: &'a Options,
    /// `--owned`, relative to the source directory (`.` for itself).
    pub owned: Vec<String>,
}

/// The exact part of the result file: the members of [`EXACT`].
pub fn exact(r: &Records) -> Vec<(String, Value)> {
    let t = &r.result.tally;
    let counters =
        Value::Object(r.set.counters.iter().map(|c| (c.name.clone(), int(c.value))).collect());
    let tally = obj(vec![
        ("selected", int(t.selected)),
        ("unparsed", int(t.unparsed)),
        ("failed_runs", int(t.failed_runs)),
        ("error_reports", int(t.error_reports)),
        ("verified", int(t.verified)),
        ("unchanged", int(t.unchanged)),
        ("mismatch", int(t.mismatch)),
        ("not_parsed", int(t.not_parsed)),
        ("unverifiable", int(t.unverifiable)),
        ("never_parsed", int(t.never_parsed)),
        ("failed", int(t.failed)),
    ]);
    let agreement = obj(vec![
        ("stale", int(r.result.agreement.len() as u64)),
        ("unused_rules", int(r.result.unused.len() as u64)),
    ]);
    let s = &r.result.spellings;
    let spellings = obj(vec![
        ("obligations", int(s.obligations)),
        ("passed", int(s.passed)),
        ("failed", int(s.failed.len() as u64)),
    ]);
    let residue = Value::Object(
        Category::ALL
            .iter()
            .map(|c| {
                let n = r.result.residue.iter().filter(|e| e.category == *c).count() as u64;
                (c.name().to_owned(), int(n))
            })
            .collect(),
    );
    vec![
        ("state".to_owned(), Value::Str(r.result.state.name().to_owned())),
        ("counters".to_owned(), counters),
        ("tally".to_owned(), tally),
        ("agreement".to_owned(), agreement),
        ("spellings".to_owned(), spellings),
        ("residue".to_owned(), residue),
    ]
}

/// The result file (`spec/harness.md` §6.5): the run's identity, the
/// checks, the exact part ([`exact`]) and the measurements (`measured`:
/// wall time, cpu time and peak resident size of the coordinator per step
/// of the transform run record, the phases' measurements, and each check's
/// wall time), which are reported and never compared.
pub fn result_json(h: &Header<'_>, r: &Records, checks: &[Check]) -> Value {
    let s = |t: &str| Value::Str(t.to_owned());
    let run = &r.run;
    let steps = run
        .steps
        .iter()
        .map(|st| {
            obj(vec![
                ("step", s(st.name.name())),
                ("wall_ns", int(st.wall_ns)),
                ("cpu_ns", int(st.cpu_ns)),
                ("peak_rss", int(st.peak_rss)),
            ])
        })
        .collect();
    let phases = r
        .phases
        .iter()
        .map(|(name, m)| {
            obj(vec![
                ("phase", s(name)),
                ("wall_ns", int(m.wall_ns)),
                ("cpu_ns", int(m.cpu_ns)),
                ("peak_rss_max", int(m.peak_rss_max)),
                ("peak_rss_sum", int(m.peak_rss_sum)),
                ("workers_max", int(m.workers_max)),
                ("cores", int(m.cores)),
                ("memory_budget", int(m.memory_budget)),
            ])
        })
        .collect();
    let check_times = checks
        .iter()
        .map(|c| obj(vec![("check", s(c.name)), ("wall_ns", int(c.wall_ns))]))
        .collect();
    let o = h.opts;
    let mut doc = vec![
        ("corpus".to_owned(), s(&h.corpus.name)),
        ("pin".to_owned(), s(&h.corpus.tag)),
        ("from".to_owned(), s(&o.from)),
        ("to".to_owned(), s(&o.to)),
        ("kind".to_owned(), o.kind.as_deref().map_or(Value::Null, s)),
        (
            "options".to_owned(),
            obj(vec![
                ("owned", Value::Array(h.owned.iter().map(|p| s(p)).collect())),
                ("expect", o.expect.map_or(Value::Null, |e| s(e.name()))),
                ("accept", Value::Bool(o.accept)),
                ("build", Value::Bool(o.build)),
                ("jobs", o.jobs.map_or(Value::Null, |j| int(j as u64))),
            ]),
        ),
        ("tool_slip".to_owned(), s(&run.tool_slip)),
        ("tool_kiln".to_owned(), s(&run.tool_kiln)),
        ("plan".to_owned(), s(&r.result.plan.to_string())),
        ("round".to_owned(), int(r.result.round)),
        ("transform".to_owned(), s(&r.result.transform.to_string())),
        ("edits".to_owned(), s(&r.result.edits.to_string())),
        ("declared".to_owned(), id_or_null(r.result.declared)),
        ("overlay".to_owned(), id_or_null(r.result.overlay)),
        ("result".to_owned(), s(&r.result_id.to_string())),
        (
            "checks".to_owned(),
            Value::Array(
                checks
                    .iter()
                    .map(|c| {
                        obj(vec![
                            ("check", s(c.name)),
                            ("outcome", s(c.outcome.name())),
                            ("detail", s(c.outcome.detail())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ];
    doc.extend(exact(r));
    doc.push((
        "measured".to_owned(),
        obj(vec![
            ("wall_ns", int(run.wall_ns)),
            ("steps", Value::Array(steps)),
            ("phases", Value::Array(phases)),
            ("checks", Value::Array(check_times)),
        ]),
    ));
    Value::Object(doc)
}

fn compact(v: Option<&Value>) -> String {
    v.map_or("absent".to_owned(), |v| json::write_compact(v).unwrap_or_else(|e| e.to_string()))
}

/// The differences of the exact members ([`EXACT`]) between the file
/// already there (`old`) and this run's (`new`): one line per member that
/// differs, `<member>[.<name>]: <old> -> <new>`, an object compared member
/// by member. Times and sizes are not compared.
pub fn exact_differences(old: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in EXACT {
        let (a, b) = (old.get(key), new.get(key));
        match (a.and_then(Value::as_object), b.and_then(Value::as_object)) {
            (Some(ao), Some(bo)) => {
                let mut names: Vec<&str> =
                    ao.iter().chain(bo.iter()).map(|(k, _)| k.as_str()).collect();
                names.sort_unstable();
                names.dedup();
                for n in names {
                    let (x, y) = (a.and_then(|v| v.get(n)), b.and_then(|v| v.get(n)));
                    if x != y {
                        out.push(format!("{key}.{n}: {} -> {}", compact(x), compact(y)));
                    }
                }
            }
            _ if a != b => out.push(format!("{key}: {} -> {}", compact(a), compact(b))),
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Files: the copy, its comparison, sizes

/// Copies the directory `from` to `to`, which must not exist, as `cp -a`
/// would: directories, regular files with their bytes, permissions and
/// modification time, and symbolic links as links. Every file is a new
/// file whose bytes are read and written here: never a hard link, never a
/// reflink or an in-kernel copy (on this workspace's mount a replaced
/// hard-linked name can show another entry's bytes). The top-level entry
/// named `skip` is not copied. Any other kind of file is an error. Returns
/// the files and the bytes copied.
pub fn copy_tree(from: &Path, to: &Path, skip: Option<&str>) -> Result<(u64, u64), String> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(format!("{} exists already", to.display()));
    }
    let mut n = (0, 0);
    copy_dir(from, to, skip, &mut n)?;
    Ok(n)
}

fn copy_dir(from: &Path, to: &Path, skip: Option<&str>, n: &mut (u64, u64)) -> Result<(), String> {
    let meta = std::fs::metadata(from).map_err(|e| format!("{}: {e}", from.display()))?;
    std::fs::create_dir(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let mut entries = Vec::new();
    for e in std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))? {
        let e = e.map_err(|e| format!("cannot read {}: {e}", from.display()))?;
        if skip.is_some_and(|s| e.file_name() == OsStr::new(s)) {
            continue;
        }
        entries.push(e.file_name());
    }
    entries.sort();
    for name in entries {
        let (src, dst) = (from.join(&name), to.join(&name));
        let m = std::fs::symlink_metadata(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        let kind = m.file_type();
        if kind.is_dir() {
            copy_dir(&src, &dst, None, n)?;
        } else if kind.is_symlink() {
            let target = std::fs::read_link(&src).map_err(|e| format!("{}: {e}", src.display()))?;
            std::os::unix::fs::symlink(&target, &dst)
                .map_err(|e| format!("cannot create {}: {e}", dst.display()))?;
        } else if kind.is_file() {
            n.1 += copy_file(&src, &dst, &m)?;
            n.0 += 1;
        } else {
            return Err(format!("{}: not a file, a directory or a symbolic link", src.display()));
        }
    }
    // Last, so that a read-only directory could be filled.
    std::fs::set_permissions(to, meta.permissions())
        .map_err(|e| format!("cannot set the mode of {}: {e}", to.display()))
}

fn copy_file(src: &Path, dst: &Path, m: &std::fs::Metadata) -> Result<u64, String> {
    let mut input = std::fs::File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
        .map_err(|e| format!("cannot create {}: {e}", dst.display()))?;
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let k = input.read(&mut buf).map_err(|e| format!("{}: {e}", src.display()))?;
        if k == 0 {
            break;
        }
        output.write_all(&buf[..k]).map_err(|e| format!("{}: {e}", dst.display()))?;
        total += k as u64;
    }
    if let Ok(t) = m.modified() {
        output
            .set_times(std::fs::FileTimes::new().set_modified(t))
            .map_err(|e| format!("cannot set the time of {}: {e}", dst.display()))?;
    }
    drop(output);
    std::fs::set_permissions(dst, m.permissions())
        .map_err(|e| format!("cannot set the mode of {}: {e}", dst.display()))?;
    Ok(total)
}

/// The differences between the trees `a` (the source; its top-level entry
/// `skip_a` left out) and `b`: a path in one only, of another kind in the
/// other, a regular file with other bytes, a link with another target.
/// Modes and times are not compared. Returns the count of differences and
/// the first `limit` as lines, in path order.
pub fn tree_differences(
    a: &Path,
    b: &Path,
    skip_a: Option<&str>,
    limit: usize,
) -> Result<(u64, Vec<String>), String> {
    let mut d = (0u64, Vec::new());
    diff_dir(a, b, Path::new(""), skip_a, limit, &mut d)?;
    Ok(d)
}

fn listing(dir: &Path, skip: Option<&str>) -> Result<Vec<OsString>, String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let e = e.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        if skip.is_none_or(|s| e.file_name() != OsStr::new(s)) {
            v.push(e.file_name());
        }
    }
    v.sort();
    Ok(v)
}

#[derive(PartialEq, Eq)]
enum Kind {
    Dir,
    File,
    Link,
    Other,
}

fn kind_of(p: &Path) -> Result<Kind, String> {
    let t = std::fs::symlink_metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.file_type();
    Ok(if t.is_dir() {
        Kind::Dir
    } else if t.is_symlink() {
        Kind::Link
    } else if t.is_file() {
        Kind::File
    } else {
        Kind::Other
    })
}

/// Reads into `buf` until it is full or the file ends; the bytes read.
fn fill(f: &mut std::fs::File, buf: &mut [u8], path: &Path) -> Result<usize, String> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(n)
}

fn same_bytes(left: &Path, right: &Path) -> Result<bool, String> {
    let open = |p: &Path| std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()));
    let (mut fl, mut fr) = (open(left)?, open(right)?);
    let len = |f: &std::fs::File, p: &Path| {
        f.metadata().map(|m| m.len()).map_err(|e| format!("{}: {e}", p.display()))
    };
    if len(&fl, left)? != len(&fr, right)? {
        return Ok(false);
    }
    let (mut bl, mut br) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let nl = fill(&mut fl, &mut bl, left)?;
        let nr = fill(&mut fr, &mut br, right)?;
        if nl != nr || bl[..nl] != br[..nr] {
            return Ok(false);
        }
        if nl == 0 {
            return Ok(true);
        }
    }
}

fn diff_dir(
    src: &Path,
    copy: &Path,
    rel: &Path,
    skip_a: Option<&str>,
    limit: usize,
    d: &mut (u64, Vec<String>),
) -> Result<(), String> {
    let say = |d: &mut (u64, Vec<String>), line: String| {
        d.0 += 1;
        if d.1.len() < limit {
            d.1.push(line);
        }
    };
    let (la, lb) = (listing(src, skip_a)?, listing(copy, None)?);
    let mut names: Vec<&OsString> = la.iter().chain(lb.iter()).collect();
    names.sort();
    names.dedup();
    for name in names {
        let r = rel.join(name);
        let (pa, pb) = (src.join(name), copy.join(name));
        match (la.contains(name), lb.contains(name)) {
            (true, false) => say(d, format!("{}: only in the source", r.display())),
            (false, true) => say(d, format!("{}: only in the copy", r.display())),
            _ => {
                let (ka, kb) = (kind_of(&pa)?, kind_of(&pb)?);
                if ka != kb {
                    say(d, format!("{}: of another kind in the copy", r.display()));
                    continue;
                }
                match ka {
                    Kind::Dir => diff_dir(&pa, &pb, &r, None, limit, d)?,
                    Kind::File => {
                        if !same_bytes(&pa, &pb)? {
                            say(d, format!("{}: other bytes", r.display()));
                        }
                    }
                    Kind::Link => {
                        let t = |p: &Path| {
                            std::fs::read_link(p).map_err(|e| format!("{}: {e}", p.display()))
                        };
                        if t(&pa)? != t(&pb)? {
                            say(d, format!("{}: a link to another target", r.display()));
                        }
                    }
                    Kind::Other => say(d, format!("{}: not a file", r.display())),
                }
            }
        }
    }
    Ok(())
}

/// The bytes of the regular files under `dir`, its top-level entry `skip`
/// left out (what [`copy_tree`] copies).
pub fn tree_size(dir: &Path, skip: Option<&str>) -> Result<u64, String> {
    let mut total = 0u64;
    for name in listing(dir, skip)? {
        let p = dir.join(&name);
        let m = std::fs::symlink_metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if m.is_dir() {
            total += tree_size(&p, None)?;
        } else if m.is_file() {
            total += m.len();
        }
    }
    Ok(total)
}

/// The available bytes of the file system of `dir`, from the output of
/// `df -Pk <dir>`: the fourth column of its second line, in KiB.
pub fn parse_df(text: &str) -> Option<u64> {
    let line = text.lines().nth(1)?;
    let kib: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    kib.checked_mul(1024)
}

// ---------------------------------------------------------------------------
// --accept

/// The clang VFS overlay file (YAML's JSON form) that maps each `(path,
/// blob)`, both absolute, to the blob: one directory entry per directory,
/// with `use-external-names` off so that clang names the overlay path.
pub fn vfs_overlay(files: &[(Vec<u8>, PathBuf)]) -> Result<String, String> {
    let utf8 = |b: &[u8]| {
        std::str::from_utf8(b).map(str::to_owned).map_err(|_| {
            format!("{} cannot be written in a VFS overlay: not UTF-8", String::from_utf8_lossy(b))
        })
    };
    let mut dirs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (path, blob) in files {
        let p = utf8(path)?;
        let (dir, base) = p
            .rsplit_once('/')
            .filter(|(_, b)| !b.is_empty())
            .ok_or_else(|| format!("{p}: not an absolute path of a file"))?;
        let dir = if dir.is_empty() { "/".to_owned() } else { dir.to_owned() };
        let blob = utf8(blob.as_os_str().as_bytes())?;
        dirs.entry(dir).or_default().push(obj(vec![
            ("name", Value::Str(base.to_owned())),
            ("type", Value::Str("file".to_owned())),
            ("external-contents", Value::Str(blob)),
        ]));
    }
    let roots = dirs
        .into_iter()
        .map(|(dir, contents)| {
            obj(vec![
                ("name", Value::Str(dir)),
                ("type", Value::Str("directory".to_owned())),
                ("contents", Value::Array(contents)),
            ])
        })
        .collect();
    let doc = obj(vec![
        ("version", int(0)),
        ("case-sensitive", Value::Str("true".to_owned())),
        ("use-external-names", Value::Bool(false)),
        ("roots", Value::Array(roots)),
    ]);
    json::write_pretty(&doc).map_err(|e| format!("the VFS overlay: {e}"))
}

/// How one clang run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Killed,
    TimedOut,
}

impl Exit {
    fn text(&self) -> String {
        match self {
            Exit::Code(c) => format!("exit {c}"),
            Exit::Killed => "killed".to_owned(),
            Exit::TimedOut => format!("no end within {} s", CLANG_TIMEOUT.as_secs()),
        }
    }
}

/// One command of the plan as `--accept` runs it.
#[derive(Debug, Clone)]
struct Unit {
    main: String,
    clang: PathBuf,
    command: slip_model::Command,
}

fn clang_run(u: &Unit, vfs: Option<&Path>) -> Result<(Exit, String), String> {
    let mut cmd = Command::new(&u.clang);
    cmd.args(u.command.argv.iter().skip(1).map(|a| OsStr::from_bytes(a)))
        .arg("-fsyntax-only")
        .current_dir(OsStr::from_bytes(&u.command.directory));
    if let Some(v) = vfs {
        cmd.arg("-ivfsoverlay").arg(v);
    }
    for e in &u.command.env {
        cmd.env(OsStr::from_bytes(&e.name), OsStr::from_bytes(&e.value));
    }
    match run_bounded(cmd, CLANG_TIMEOUT) {
        Ok(f) => {
            let err = String::from_utf8_lossy(&f.stderr);
            let first = err
                .lines()
                .find(|l| l.contains("error"))
                .or_else(|| err.lines().next())
                .unwrap_or("")
                .to_owned();
            Ok((f.status.code().map_or(Exit::Killed, Exit::Code), first))
        }
        Err(Bounded::TimedOut { .. }) => Ok((Exit::TimedOut, String::new())),
        Err(Bounded::Spawn(e)) => Err(format!("cannot run {}: {e}", u.clang.display())),
    }
}

// ---------------------------------------------------------------------------
// The command

/// The environment the command reads.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `$SLIP_CORPUS_DIR`.
    pub corpus_dir: Option<OsString>,
    /// `$CARGO_TARGET_DIR`, for the default `slip`.
    pub target_dir: Option<OsString>,
    /// `$PATH`, where the tools are looked up (the corpus's compilers,
    /// `cmake`, `make`, `patch`, `df`).
    pub path: Option<OsString>,
}

/// `cargo xtask corpus rename …` with the process's environment.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    let env = Env {
        corpus_dir: std::env::var_os("SLIP_CORPUS_DIR"),
        target_dir: std::env::var_os("CARGO_TARGET_DIR"),
        path: std::env::var_os("PATH"),
    };
    let mut stdout = std::io::stdout();
    match run(repo, args, &env, &mut stdout) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Failure::Usage(e)) => {
            eprintln!("xtask corpus rename: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask corpus rename: {e}");
            ExitCode::from(1)
        }
    }
}

/// Runs `args` (after `rename`), writing the lines to `out`. `Ok(false)`:
/// a check failed, or the counters differ from the file already there.
pub fn run(repo: &Path, args: &[String], env: &Env, out: &mut dyn Write) -> Result<bool, Failure> {
    let (name, opts) = parse_args(args).map_err(Failure::Usage)?;
    let target = target_name(&opts.from).map_err(Failure::Usage)?;
    if !identifier(&opts.to) {
        return Err(Failure::Usage(format!("`--to {}` is not an identifier", opts.to)));
    }
    let e = Failure::Error;
    let corpora = rows::load(repo).map_err(e)?;
    let c = rows::find(&corpora, &name).map_err(e)?;
    let root = corpus::corpus_root(repo, env.corpus_dir.as_deref()).map_err(e)?;
    let layout = Layout::new(&root, &c.name);
    let s = corpus::status(c, &layout);
    if !(s.configured && s.generated && matches!(s.database, Some(Ok(_)))) {
        return Err(Failure::Error(format!(
            "corpus `{}` is not configured: run `cargo xtask corpus configure {}`",
            c.name, c.name
        )));
    }
    let slip =
        slip_run::find_slip(repo, opts.slip.as_deref(), env.target_dir.as_deref()).map_err(e)?;
    let paths = Paths::new(&layout.dir, &target);
    let r = Rename { repo, c, layout, paths, opts: &opts, target, slip, env };
    r.execute(out).map_err(Failure::Error)
}

struct Rename<'a> {
    repo: &'a Path,
    c: &'a Corpus,
    layout: Layout,
    paths: Paths,
    opts: &'a Options,
    target: String,
    slip: PathBuf,
    env: &'a Env,
}

fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn secs(t: Instant) -> String {
    format!("{:.1}s", t.elapsed().as_secs_f64())
}

fn ns(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Runs `slip` with `args`, its standard output copied line by line to
/// `out` (each line after `prefix`) as it comes, and kept.
fn run_streamed(
    slip: &Path,
    args: &[OsString],
    prefix: &str,
    out: &mut dyn Write,
) -> Result<Ran, String> {
    use std::os::unix::process::ExitStatusExt;
    let shown = slip.display();
    let mut child = Command::new(slip)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("cannot run {shown}: {e}"))?;
    let mut text = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        let mut r = std::io::BufReader::new(pipe);
        loop {
            let mut line = Vec::new();
            let n = r.read_until(b'\n', &mut line).map_err(|e| format!("{shown}: {e}"))?;
            if n == 0 {
                break;
            }
            let _ = out.write_all(prefix.as_bytes());
            let _ = out.write_all(&line);
            if !line.ends_with(b"\n") {
                let _ = out.write_all(b"\n");
            }
            let _ = out.flush();
            text.extend_from_slice(&line);
        }
    }
    let status = child.wait().map_err(|e| format!("{shown}: {e}"))?;
    let status = match (status.code(), status.signal()) {
        (Some(code), _) => code,
        (None, Some(sig)) => return Err(format!("{shown} was killed by signal {sig}")),
        (None, None) => return Err(format!("{shown} ended without an exit status")),
    };
    let stdout =
        String::from_utf8(text).map_err(|e| format!("the output of {shown} is not UTF-8: {e}"))?;
    Ok(Ran { status, stdout })
}

/// The meaning of an exit status of `slip rename` without a result
/// (`spec/edits.md` §10.5).
fn status_meaning(status: i32) -> &'static str {
    match status {
        2 => "a usage error",
        3 => "an input error",
        _ => "not a status of `slip rename`",
    }
}

impl Rename<'_> {
    fn tool(&self, name: &str) -> Result<PathBuf, String> {
        corpus::find_tool(name, self.env.path.as_deref())
    }

    /// `--owned` as real paths, each under the source directory; the
    /// source directory itself without one.
    fn owned(&self) -> Result<Vec<PathBuf>, String> {
        let src = std::fs::canonicalize(&self.layout.src)
            .map_err(|e| format!("{}: {e}", self.layout.src.display()))?;
        if self.opts.owned.is_empty() {
            return Ok(vec![src]);
        }
        self.opts
            .owned
            .iter()
            .map(|p| {
                let real = std::fs::canonicalize(p)
                    .map_err(|e| format!("--owned {}: {e}", p.display()))?;
                if real.starts_with(&src) {
                    Ok(real)
                } else {
                    Err(format!(
                        "--owned {}: not under the corpus's source directory {}",
                        p.display(),
                        src.display()
                    ))
                }
            })
            .collect()
    }

    fn jobs_args(&self) -> Vec<OsString> {
        self.opts.jobs.map_or_else(Vec::new, |j| vec!["-j".into(), j.to_string().into()])
    }

    fn kind_args(&self) -> Vec<OsString> {
        self.opts.kind.as_ref().map_or_else(Vec::new, |k| vec!["--kind".into(), k.into()])
    }

    /// Before anything runs, for `--build`: the tools are there, the free
    /// space is at least twice what is copied, and the copy and build tree
    /// of an earlier run are removed.
    fn prepare_build(&self, out: &mut dyn Write) -> Result<(), String> {
        for t in ["cmake", "make", "patch", "df"] {
            self.tool(t)?;
        }
        self.tool(&self.c.c_compiler)?;
        self.tool(&self.c.cxx_compiler)?;
        let size = tree_size(&self.layout.src, Some(NOT_COPIED))?;
        let df = Command::new(self.tool("df")?)
            .arg("-Pk")
            .arg(&self.layout.dir)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run df: {e}"))?;
        let free = parse_df(&String::from_utf8_lossy(&df.stdout)).ok_or_else(|| {
            format!("`df -Pk {}` printed no available size", self.layout.dir.display())
        })?;
        if free / 2 < size {
            return Err(format!(
                "--build: {} has {free} bytes free, less than twice the {size} bytes of {} that \
                 are copied; nothing was run",
                self.layout.dir.display(),
                self.layout.src.display()
            ));
        }
        for p in [&self.paths.copy, &self.paths.build] {
            if std::fs::symlink_metadata(p).is_ok() {
                say(out, &format!("build: removing {} of an earlier run", p.display()));
                slip_run::remove_tree(&self.layout.dir, p)?;
            }
        }
        Ok(())
    }

    fn execute(&self, out: &mut dyn Write) -> Result<bool, String> {
        say(out, &format!("corpus rename {}: {} to {}", self.c.name, self.opts.from, self.opts.to));
        let owned = self.owned()?;
        if self.opts.build {
            self.prepare_build(out)?;
        }
        let ran = self.rename(&owned, out).map_err(|e| format!("rename: {e}"))?;
        let records = self.records(&ran).map_err(|e| format!("rename: {e}"))?;

        let mut checks: Vec<Check> = Vec::new();
        let gate = |checks: &mut Vec<Check>,
                    out: &mut dyn Write,
                    name: &'static str,
                    f: &mut dyn FnMut(&mut dyn Write) -> Outcome| {
            let failed = checks.iter().any(|c| matches!(c.outcome, Outcome::Fail(_)));
            let t = Instant::now();
            let outcome = if failed { Outcome::NotRun } else { f(out) };
            let c = Check { name, outcome, wall_ns: ns(t) };
            say(out, &c.line());
            checks.push(c);
        };
        let state = records.result.state;
        // A refused result has no overlay and `slip` wrote no patch: there
        // is no post-image to compile, build or rename back.
        let no_image = records
            .result
            .overlay
            .is_none()
            .then(|| format!("the result is {}: there is no post-image", state.name()));
        gate(&mut checks, out, "state", &mut |_| check_state(state, self.opts.expect));
        gate(&mut checks, out, "determinism", &mut |out| self.determinism(&records, out));
        gate(&mut checks, out, "accept", &mut |out| {
            if let Some(why) = &no_image {
                Outcome::Skipped(why.clone())
            } else if self.opts.accept {
                self.accept(&records, out).unwrap_or_else(Outcome::Fail)
            } else {
                Outcome::Skipped("not asked: --accept".to_owned())
            }
        });
        let mut made = false;
        gate(&mut checks, out, "build", &mut |out| {
            if let Some(why) = &no_image {
                Outcome::Skipped(why.clone())
            } else if self.opts.build {
                made = true;
                self.build(out).unwrap_or_else(Outcome::Fail)
            } else {
                Outcome::Skipped("not asked: --build".to_owned())
            }
        });
        gate(&mut checks, out, "back", &mut |out| {
            if let Some(why) = &no_image {
                Outcome::Skipped(why.clone())
            } else if self.opts.build {
                self.back(&owned, out).unwrap_or_else(Outcome::Fail)
            } else {
                Outcome::Skipped("not asked: --build".to_owned())
            }
        });
        let passed = !checks.iter().any(|c| matches!(c.outcome, Outcome::Fail(_)));
        if made {
            for p in [&self.paths.copy, &self.paths.build] {
                if passed {
                    slip_run::remove_tree(&self.layout.dir, p)?;
                    say(out, &format!("removed {}", p.display()));
                } else if std::fs::symlink_metadata(p).is_ok() {
                    say(out, &format!("kept {}", p.display()));
                }
            }
        }
        let same = self.write_result(&records, &checks, passed, out)?;
        let failed: Vec<&str> = checks
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Fail(_)))
            .map(|c| c.name)
            .chain((!same).then_some("counters"))
            .collect();
        if failed.is_empty() {
            say(out, "corpus rename: ok");
        } else {
            say(out, &format!("corpus rename: FAILED ({})", failed.join(", ")));
        }
        Ok(failed.is_empty())
    }

    /// `slip rename` over the corpus's database, writing the patch.
    fn rename(&self, owned: &[PathBuf], out: &mut dyn Write) -> Result<Ran, String> {
        let p = &self.paths;
        std::fs::create_dir_all(&p.files)
            .map_err(|e| format!("cannot create {}: {e}", p.files.display()))?;
        match std::fs::remove_file(&p.patch) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("cannot remove {}: {e}", p.patch.display()));
            }
            _ => {}
        }
        let mut args: Vec<OsString> = vec![
            "rename".into(),
            "--from".into(),
            (&self.opts.from).into(),
            "--to".into(),
            (&self.opts.to).into(),
        ];
        args.extend(self.kind_args());
        args.extend(["--db".into(), self.layout.database().into()]);
        for o in owned {
            args.extend(["--owned".into(), o.into()]);
        }
        args.extend([
            "--store".into(),
            p.store.clone().into(),
            "--allow-unparsed".into(),
            "--allow-residue".into(),
            "--patch".into(),
            p.patch.clone().into(),
            "--relative-to".into(),
            self.layout.src.clone().into(),
        ]);
        args.extend(self.jobs_args());
        let t = Instant::now();
        say(out, &format!("step rename: started (slip rename, store {})", p.store.display()));
        let ran = run_streamed(&self.slip, &args, "", out)?;
        say(out, &format!("step rename: done in {} (exit status {})", secs(t), ran.status));
        if !matches!(ran.status, 0 | 1 | 4) {
            return Err(format!(
                "`slip rename` exited with status {}, {}",
                ran.status,
                status_meaning(ran.status)
            ));
        }
        Ok(ran)
    }

    fn records(&self, ran: &Ran) -> Result<Records, String> {
        let lines = Lines::read(&ran.stdout);
        let id = lines.result.as_deref().ok_or("`slip rename` printed no `result <id>` line")?;
        let r = read_records(&self.paths.store, id)?;
        let printed = lines.state.as_deref().unwrap_or("none");
        if printed != r.result.state.name() {
            return Err(format!(
                "`slip rename` printed state {printed}, its result {id} says {}",
                r.result.state.name()
            ));
        }
        Ok(r)
    }

    /// Check 2: `slip reduce` over the round's shards gives the run's edit
    /// set id.
    fn determinism(&self, r: &Records, out: &mut dyn Write) -> Outcome {
        let mut args: Vec<OsString> = vec![
            "reduce".into(),
            "--plan".into(),
            r.result.plan.to_string().into(),
            "--round".into(),
            r.result.round.to_string().into(),
            "--store".into(),
            self.paths.store.clone().into(),
        ];
        args.extend(self.jobs_args());
        let t = Instant::now();
        say(out, "step reduce: started (slip reduce of the same round)");
        let ran = match run_streamed(&self.slip, &args, "reduce| ", out) {
            Ok(r) => r,
            Err(e) => return Outcome::Fail(format!("reduce: {e}")),
        };
        say(out, &format!("step reduce: done in {} (exit status {})", secs(t), ran.status));
        if !matches!(ran.status, 0 | 1) {
            return Outcome::Fail(format!("`slip reduce` exited with status {}", ran.status));
        }
        let want = r.result.edits.to_string();
        match Lines::read(&ran.stdout).edits {
            Some(id) if id == want => Outcome::Ok(format!("edits {id}")),
            Some(id) => {
                Outcome::Fail(format!("`slip reduce` gave the edit set {id}, the run {want}"))
            }
            None => Outcome::Fail("`slip reduce` printed no `edits <id>` line".to_owned()),
        }
    }

    /// Check 3: every command of the plan, by the corpus's compiler with
    /// `-fsyntax-only`, exits on the post-image (through a VFS overlay of
    /// the overlay's changed files) as it does on the pre-image.
    fn accept(&self, r: &Records, out: &mut dyn Write) -> Result<Outcome, String> {
        let store = &self.paths.store;
        let Some(ov_id) = r.result.overlay else {
            return Err("the result has no overlay".to_owned());
        };
        let (_, overlay) =
            read_object(store, "overlay", ("overlays", "ovly"), &ov_id, Overlay::from_bytes)?;
        let st = Store::open(store, Access::Shared)
            .map_err(|e| format!("{}: cannot open the store: {e}", store.display()))?;
        let plan_bytes = st
            .read_plan(&r.result.plan)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{}: no plan {}", store.display(), r.result.plan))?;
        let plan = Plan::from_bytes(&plan_bytes).map_err(|e| format!("the plan: {e}"))?;
        let mut mapped = Vec::new();
        for f in overlay.files.iter().filter(|f| f.post.content != f.pre.content) {
            let blob = st.blob_path(&f.post.content);
            if !blob.is_file() {
                return Err(format!("{}: the post-image blob is missing", blob.display()));
            }
            mapped.push((f.path.clone(), blob));
        }
        let text = vfs_overlay(&mapped)?;
        std::fs::write(&self.paths.vfs, text)
            .map_err(|e| format!("cannot write {}: {e}", self.paths.vfs.display()))?;
        let cc = self.tool(&self.c.c_compiler)?;
        let cxx = self.tool(&self.c.cxx_compiler)?;
        let units: Vec<Unit> = plan
            .observers
            .iter()
            .map(|o| Unit {
                main: String::from_utf8_lossy(&o.main.path).into_owned(),
                clang: if matches!(o.language, Language::C | Language::ObjC) {
                    cc.clone()
                } else {
                    cxx.clone()
                },
                command: o.command.clone(),
            })
            .collect();
        let pool = Pool::from_env();
        let jobs = self.opts.jobs.unwrap_or_else(|| pool.jobs()).max(1);
        let pool = pool.with_jobs(jobs);
        let t = Instant::now();
        say(
            out,
            &format!(
                "step accept: started ({} commands, each on both images; at most {jobs} at once; \
                 overlay {})",
                units.len(),
                self.paths.vfs.display()
            ),
        );
        let results = run_units(&units, &self.paths.vfs, &pool, jobs, out)?;
        say(out, &format!("step accept: done in {}", secs(t)));
        let mut both = 0u64;
        let mut differ = Vec::new();
        for (u, ((pre, _), (post, why))) in units.iter().zip(&results) {
            if pre == post && matches!(pre, Exit::Code(_)) {
                both += u64::from(*pre != Exit::Code(0));
            } else {
                differ.push(format!(
                    "  {}: pre-image {}, post-image {}{}",
                    u.main,
                    pre.text(),
                    post.text(),
                    if why.is_empty() { String::new() } else { format!(": {why}") }
                ));
            }
        }
        for l in differ.iter().take(LISTED) {
            say(out, l);
        }
        Ok(if differ.is_empty() {
            Outcome::Ok(format!(
                "{} commands; {both} rejected on both images, none otherwise",
                units.len()
            ))
        } else {
            Outcome::Fail(format!(
                "{} of {} commands end otherwise on the post-image",
                differ.len(),
                units.len()
            ))
        })
    }

    /// Check 4: the patch applied to a copy of the source directory, which
    /// is configured and generated as the corpus is and built (`all`).
    fn build(&self, out: &mut dyn Write) -> Result<Outcome, String> {
        let p = &self.paths;
        let t = Instant::now();
        say(
            out,
            &format!("step copy: started ({} to {})", self.layout.src.display(), p.copy.display()),
        );
        let (files, bytes) = copy_tree(&self.layout.src, &p.copy, Some(NOT_COPIED))
            .map_err(|e| format!("copy: {e}"))?;
        say(out, &format!("step copy: done in {} ({files} files, {bytes} bytes)", secs(t)));
        std::fs::create_dir_all(&p.build)
            .map_err(|e| format!("cannot create {}: {e}", p.build.display()))?;
        let mut ctx = Ctx {
            layout: Layout {
                dir: self.layout.dir.clone(),
                src: p.copy.clone(),
                build: p.build.clone(),
            },
            path: self.env.path.clone(),
            out,
        };
        let t = Instant::now();
        ctx.say(&format!("step patch: started ({})", p.patch.display()));
        let size = std::fs::metadata(&p.patch)
            .map_err(|e| format!("patch: slip wrote no patch {}: {e}", p.patch.display()))?
            .len();
        if size > 0 {
            let patch = self.tool("patch")?;
            if let Err(e) = corpus::run_logged(
                Command::new(&patch)
                    .args(["-p0", "--batch", "--forward", "--no-backup-if-mismatch", "-i"])
                    .arg(&p.patch)
                    .current_dir(&p.copy),
                "patch",
                &p.build.join("patch.log"),
                &mut ctx,
            ) {
                return Ok(Outcome::Fail(format!("patch: {e}")));
            }
        }
        ctx.say(&format!("step patch: done in {}", secs(t)));
        let cc = self.tool(&self.c.c_compiler)?;
        let cxx = self.tool(&self.c.cxx_compiler)?;
        let args = corpus::cmake_args(self.c, &ctx.layout, &cc, &cxx);
        let make = self.tool("make")?;
        let log = p.build.join("configure.log");
        let t = Instant::now();
        ctx.say(&format!("step configure: started (cmake; log {})", log.display()));
        let mut cmake = Command::new(self.tool("cmake")?);
        cmake.args(&args).current_dir(&p.build).env("PYTHONDONTWRITEBYTECODE", "1");
        if let Err(e) = corpus::run_logged(&mut cmake, "cmake", &log, &mut ctx) {
            return Ok(Outcome::Fail(format!("configure: {e}")));
        }
        ctx.say(&format!("step configure: done in {}", secs(t)));
        let plan = match build_tree::plan(&p.build, &self.c.generate) {
            Ok(plan) => plan,
            Err(e) => return Ok(Outcome::Fail(format!("generate: {e}"))),
        };
        let log = p.build.join("generate.log");
        let t = Instant::now();
        ctx.say(&format!("step generate: started (jobs from MAKEFLAGS; log {})", log.display()));
        let mut makes: Vec<Vec<String>> = Vec::new();
        if !plan.targets.is_empty() {
            makes.push(plan.targets.clone());
        }
        for (makefile, file) in &plan.files {
            makes.push(vec!["-f".to_owned(), makefile.clone(), file.clone()]);
        }
        for m in makes {
            let mut cmd = Command::new(&make);
            cmd.args(&m).current_dir(&p.build).env("PYTHONDONTWRITEBYTECODE", "1");
            if let Err(e) = corpus::run_logged(&mut cmd, "make", &log, &mut ctx) {
                return Ok(Outcome::Fail(format!("generate: {e}")));
            }
        }
        ctx.say(&format!("step generate: done in {}", secs(t)));
        let log = p.build.join("build.log");
        let t = Instant::now();
        ctx.say(&format!(
            "step build: started (make all; jobs from MAKEFLAGS; log {})",
            log.display()
        ));
        let mut cmd = Command::new(&make);
        cmd.arg("all").current_dir(&p.build).env("PYTHONDONTWRITEBYTECODE", "1");
        if let Err(e) = corpus::run_logged(&mut cmd, "make all", &log, &mut ctx) {
            return Ok(Outcome::Fail(format!("build: {e}")));
        }
        ctx.say(&format!("step build: done in {}", secs(t)));
        if !ctx.layout.database().is_file() {
            return Ok(Outcome::Fail(format!(
                "configure: cmake wrote no {}",
                ctx.layout.database().display()
            )));
        }
        Ok(Outcome::Ok(format!(
            "patched, configured, generated and built in {}",
            p.build.display()
        )))
    }

    /// Check 5: the inverse rename over the copy's database, with
    /// `--write`, leaves the copy byte-identical to the source directory.
    fn back(&self, owned: &[PathBuf], out: &mut dyn Write) -> Result<Outcome, String> {
        let p = &self.paths;
        let run: Vec<String> = ["rename", "--from", &self.opts.from, "--to", &self.opts.to]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let inverse = cmdline::inverse(&run).ok_or_else(|| {
            format!("no inverse of `--from {} --to {}`", self.opts.from, self.opts.to)
        })?;
        let src = std::fs::canonicalize(&self.layout.src)
            .map_err(|e| format!("{}: {e}", self.layout.src.display()))?;
        let mut args: Vec<OsString> = inverse.iter().map(OsString::from).collect();
        args.extend(self.kind_args());
        args.extend(["--db".into(), p.build.join("compile_commands.json").into()]);
        for o in owned {
            let rel = o.strip_prefix(&src).unwrap_or(Path::new(""));
            args.extend(["--owned".into(), p.copy.join(rel).into()]);
        }
        args.extend([
            "--store".into(),
            p.back_store.clone().into(),
            "--allow-unparsed".into(),
            "--allow-residue".into(),
            "--write".into(),
        ]);
        args.extend(self.jobs_args());
        let t = Instant::now();
        say(
            out,
            &format!("step back: started (slip {} over the copy, --write)", inverse[1..].join(" ")),
        );
        let ran = run_streamed(&self.slip, &args, "back| ", out)?;
        say(out, &format!("step back: done in {} (exit status {})", secs(t), ran.status));
        if !matches!(ran.status, 0 | 1) {
            return Ok(Outcome::Fail(format!(
                "the inverse rename exited with status {}",
                ran.status
            )));
        }
        let state = Lines::read(&ran.stdout).state.unwrap_or_else(|| "none".to_owned());
        let t = Instant::now();
        say(out, "step compare: started (every file of the copy against the source)");
        let (n, lines) = tree_differences(&self.layout.src, &p.copy, Some(NOT_COPIED), LISTED)?;
        say(out, &format!("step compare: done in {}", secs(t)));
        for l in &lines {
            say(out, &format!("  {l}"));
        }
        Ok(if n == 0 {
            Outcome::Ok(format!("the inverse rename ended {state}; the copy is the source again"))
        } else {
            Outcome::Fail(format!(
                "the inverse rename ended {state}; {n} difference(s) from the source"
            ))
        })
    }

    /// Writes the result file: replaces `bench/results/<corpus>.rename.
    /// <target>.json` when every check passed and its exact members equal
    /// those of the file there (or there is none); otherwise writes
    /// `<file>.new` beside it and leaves the file. Returns whether the exact
    /// members are the file's (true when there was none).
    fn write_result(
        &self,
        r: &Records,
        checks: &[Check],
        passed: bool,
        out: &mut dyn Write,
    ) -> Result<bool, String> {
        let src = std::fs::canonicalize(&self.layout.src)
            .map_err(|e| format!("{}: {e}", self.layout.src.display()))?;
        let owned = self
            .owned()?
            .iter()
            .map(|o| {
                let rel = o.strip_prefix(&src).unwrap_or(o).display().to_string();
                if rel.is_empty() { ".".to_owned() } else { rel }
            })
            .collect();
        let h = Header { corpus: self.c, opts: self.opts, owned };
        let doc = result_json(&h, r, checks);
        let text = json::write_pretty(&doc).map_err(|e| format!("the result file: {e}"))?;
        let path = result_path(self.repo, &self.c.name, &self.target);
        let shown = path.strip_prefix(self.repo).unwrap_or(&path).display().to_string();
        let same = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                say(out, &format!("counters: new ({shown} does not exist)"));
                true
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
            Ok(bytes) => {
                let old = json::parse(&bytes)
                    .map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
                let d = exact_differences(&old, &doc);
                if d.is_empty() {
                    say(out, &format!("counters: the same as {shown}"));
                } else {
                    say(out, &format!("counters: DIFFERENT from {shown} ({} member(s))", d.len()));
                    for l in &d {
                        say(out, &format!("  {l}"));
                    }
                }
                d.is_empty()
            }
        };
        let dir = path.parent().unwrap_or(self.repo);
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let new = new_path(&path);
        if passed && same {
            write_file(&path, &text)?;
            match std::fs::remove_file(&new) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(format!("cannot remove {}: {e}", new.display()));
                }
                _ => {}
            }
            say(out, &format!("result {shown}"));
        } else {
            write_file(&new, &text)?;
            say(out, &format!("result {shown}.new ({shown} not replaced)"));
        }
        Ok(same)
    }
}

/// Writes `text` to `path` through `<path>.tmp` and a rename.
fn write_file(path: &Path, text: &str) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Runs every unit on the pre-image and then on the post-image (`vfs`),
/// each pair under one jobserver token, at most `jobs` pairs at once;
/// prints a line at each tenth done. In the order of `units`.
#[allow(clippy::type_complexity, reason = "a pair of (exit, first error line) per unit")]
fn run_units(
    units: &[Unit],
    vfs: &Path,
    pool: &Pool,
    jobs: usize,
    out: &mut dyn Write,
) -> Result<Vec<((Exit, String), (Exit, String))>, String> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut results: Vec<Option<((Exit, String), (Exit, String))>> = vec![None; units.len()];
    let mut first_error: Option<String> = None;
    std::thread::scope(|s| {
        for _ in 0..jobs.min(units.len()) {
            let (tx, next) = (tx.clone(), &next);
            s.spawn(move || {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(u) = units.get(i) else { break };
                    let token = pool.acquire();
                    let pair =
                        clang_run(u, None).and_then(|pre| Ok((pre, clang_run(u, Some(vfs))?)));
                    drop(token);
                    if tx.send((i, pair)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut done = 0usize;
        let n = units.len();
        for (i, pair) in rx {
            done += 1;
            match pair {
                Ok(p) => results[i] = Some(p),
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    // Nothing more is started: every worker stops at its next unit.
                    next.store(n, std::sync::atomic::Ordering::Relaxed);
                }
            }
            if done * 10 / n.max(1) > (done - 1) * 10 / n.max(1) {
                say(out, &format!("accept: {done} of {n} commands"));
            }
        }
    });
    if let Some(e) = first_error {
        return Err(e);
    }
    results
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.ok_or_else(|| format!("the command of {} was not run", units[i].main)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn df_output() {
        let text = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
                    /dev/vda1        100000000  40000000  60000000      40% /\n";
        assert_eq!(parse_df(text), Some(60_000_000 * 1024));
        assert_eq!(parse_df("Filesystem\n"), None);
        assert_eq!(parse_df(""), None);
    }

    #[test]
    fn target_names() {
        assert_eq!(target_name("llvm::getAsUnsignedInteger").unwrap(), "llvm.getAsUnsignedInteger");
        assert_eq!(target_name("::a::b_2").unwrap(), "a.b_2");
        assert_eq!(target_name("f").unwrap(), "f");
        for bad in ["", "::", "a::", "a::::b", "a/b", "a::1b", "../x", "a.b"] {
            assert!(target_name(bad).is_err(), "{bad}");
        }
    }
}
