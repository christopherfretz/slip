//! `cargo xtask census <corpus>` (`spec/harness.md` §9.5): the causes of
//! indeterminate units over a corpus, before a run that needs none.
//!
//! ```text
//! cargo xtask census <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
//!                    [--store <dir>]
//! ```
//!
//! Runs `slip parse` into `<corpus dir>/stores/census` (or
//! `<corpus dir>/stores/census-<hex16>` for a `--only`/`--limit` selection,
//! or `--store <dir>`), or reuses the newest map run already in that store
//! when it is the exact run recorded in `census.json` and its plan, Kiln
//! pin, `slip` binary digest, tool versions, selection, and every reused
//! shard's recorded positive and negative inputs on disk all match. Reads
//! every selected observer's own map shard through `perf::load` and
//! `invariants::side`, writes `<store>/census.json`, and prints per-cause
//! counts (with the first five observers by path), per-cause text counts,
//! unparsed (`diagnosed`, `rejected`, not counted toward uncleanliness) and
//! unclean non-`ok` (`failed`, `incomplete`, `no_digests`) shards by status,
//! and the verdict line.
//!
//! Exit status: 0 clean, 1 not clean, 2 for an error of the command.

use crate::corpus::{self, Corpus, Layout, rows};
use crate::invariants::{self, Env, Failure, Side, latest_map_run, run_slip, side, versions};
use crate::perf::{self, Loaded, render::text};
use crate::slip_run;
use slip_model::hash::content_digest;
use slip_model::json::{self, Value};
use slip_model::{Digest, Status};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask census <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
                   [--store <dir>]";

/// The JSON format version written to `<store>/census.json`.
pub const FORMAT: &str = "slip.census.v1";

/// At most this many sample observers (by path) are listed per cause.
pub const SAMPLE_OBSERVERS: usize = 5;

// ---------------------------------------------------------------------------
// Options

/// The options after `census <corpus>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub common: invariants::Options,
    pub store: Option<PathBuf>,
}

/// Parses `<corpus>` and options; `Err` is a usage error.
pub fn parse_args(args: &[String]) -> Result<(String, Options), String> {
    let (corpus, rest) = match args {
        [c, rest @ ..] if !c.starts_with('-') => (c.clone(), rest),
        _ => return Err("missing corpus".to_owned()),
    };
    let mut o = Options::default();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("`{arg}` needs a value"));
        match arg.as_str() {
            "--slip" if o.common.slip.is_none() => o.common.slip = Some(PathBuf::from(value()?)),
            "--only" => o.common.only.push(value()?),
            "--limit" if o.common.limit.is_none() => {
                let v = value()?;
                let n = (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| v.parse::<u64>().ok())
                    .flatten()
                    .ok_or_else(|| format!("`--limit {v}` is not a number"))?;
                o.common.limit = Some((v, n));
            }
            "-j" if o.common.jobs.is_none() => o.common.jobs = Some(value()?),
            "--store" if o.store.is_none() => o.store = Some(PathBuf::from(value()?)),
            "--slip" | "--limit" | "-j" | "--store" => {
                return Err(format!("`{arg}` is given twice"));
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok((corpus, o))
}

// ---------------------------------------------------------------------------
// Pure aggregation

/// One `digest.indeterminate.<cause>` row (including `pending.<area>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseRow {
    pub cause: String,
    pub keys: u64,
    pub observers: u64,
    /// First [`SAMPLE_OBSERVERS`] observers with at least one key of this
    /// cause, sorted by main file path bytes.
    pub sample_observers: Vec<Vec<u8>>,
}

/// One `digest.cause.<cause>.<word>` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseTextRow {
    pub name: String,
    pub keys: u64,
    pub observers: u64,
}

/// The census computed over one map run and a selection of observers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    pub observers: u64,
    /// Plan observers left out by the selection (`plan.observers.len() - observers`).
    pub omitted: u64,
    pub ok: u64,
    /// Unparsed observers (`diagnosed` and `rejected`): listed with their
    /// counts, not counted toward uncleanliness.
    pub unparsed: u64,
    /// Unparsed observers grouped by status (`diagnosed`, `rejected`).
    pub unparsed_by_status: BTreeMap<String, u64>,
    /// Unclean non-`ok` observers (`failed`, `incomplete`, `no_digests`).
    pub not_ok: u64,
    /// Unclean non-`ok` observers grouped by status (`failed`, `incomplete`,
    /// `no_digests`).
    pub not_ok_by_status: BTreeMap<String, u64>,
    /// Total indeterminate keys across `ok` observers without double-counting
    /// dotted sub-causes alongside their top-level cause.
    pub indeterminate_keys: u64,
    /// Number of `ok` observers with at least one indeterminate key or cause.
    pub indeterminate_observers: u64,
    /// Per-cause rows (`digest.indeterminate.<cause>`), ordered by `keys`
    /// descending then `cause` ascending.
    pub causes: Vec<CauseRow>,
    /// Per-cause-text rows (`digest.cause.<cause>.<word>`), ordered by `keys`
    /// descending then `name` ascending.
    pub cause_texts: Vec<CauseTextRow>,
}

impl Census {
    /// Whether the corpus (or selection) is clean: at least one selected
    /// observer, no unclean shards (`failed`, `incomplete`, `no_digests`),
    /// zero indeterminate keys and observers, and no indeterminate causes.
    /// Unparsed observers (`diagnosed`, `rejected`) do not make the census
    /// unclean.
    pub fn is_clean(&self) -> bool {
        self.observers > 0
            && self.not_ok == 0
            && self.indeterminate_keys == 0
            && self.indeterminate_observers == 0
            && self.causes.is_empty()
    }

    /// The last line printed by `cargo xtask census` (and echoed by
    /// `cargo xtask corpus rename`).
    pub fn last_line(&self, selection: bool) -> String {
        let suffix = if selection { " (selection)" } else { "" };
        let unparsed = if self.unparsed > 0 {
            format!("; {} unparsed (not counted)", self.unparsed)
        } else {
            String::new()
        };
        if self.is_clean() {
            format!("census: clean ({} observers{unparsed}){suffix}", self.observers)
        } else if self.observers == 0 {
            format!(
                "census: 0 indeterminate keys in 0 of 0 observers, 0 causes; 0 not ok \
                 (no observer selected){suffix}"
            )
        } else {
            format!(
                "census: {} indeterminate keys in {} of {} observers, {} causes; {} not ok\
                 {unparsed}{suffix}",
                self.indeterminate_keys,
                self.indeterminate_observers,
                self.observers,
                self.causes.len(),
                self.not_ok
            )
        }
    }
}

fn add(total: &mut u64, n: u64, what: &str) -> Result<(), String> {
    *total = total.checked_add(n).ok_or_else(|| format!("{what} overflows a 64-bit integer"))?;
    Ok(())
}

/// Aggregates one `ok` shard's indeterminate and cause-text counters into
/// `causes_map` and `texts_map`, returning `(indeterminate_keys, any_indet)`.
#[allow(clippy::type_complexity, reason = "local aggregation map for summarize")]
fn tally_ok_shard(
    shard: &slip_model::Shard,
    main_path: &[u8],
    causes_map: &mut BTreeMap<String, (u64, u64, Vec<Vec<u8>>)>,
    texts_map: &mut BTreeMap<String, (u64, u64)>,
) -> Result<(u64, bool), String> {
    let mut total_counter = 0u64;
    let mut top_sum = 0u64;
    let mut top_causes: BTreeSet<&str> = BTreeSet::new();
    let mut sub_causes: Vec<(&str, u64)> = Vec::new();
    let mut any_indet = false;

    for c in &shard.counters {
        if c.value == 0 {
            continue;
        }
        if c.name == "digest.indeterminate" {
            total_counter = c.value;
            any_indet = true;
        } else if let Some(cause) = c.name.strip_prefix("digest.indeterminate.") {
            any_indet = true;
            let entry = causes_map.entry(cause.to_owned()).or_insert_with(|| (0, 0, Vec::new()));
            add(&mut entry.0, c.value, &c.name)?;
            entry.1 += 1;
            entry.2.push(main_path.to_vec());
            if let Some((parent, _)) = cause.split_once('.') {
                sub_causes.push((parent, c.value));
            } else {
                top_causes.insert(cause);
                add(&mut top_sum, c.value, "indeterminate_keys")?;
            }
        } else if let Some(name) = c.name.strip_prefix("digest.cause.") {
            let entry = texts_map.entry(name.to_owned()).or_insert((0, 0));
            add(&mut entry.0, c.value, &c.name)?;
            entry.1 += 1;
        }
    }

    let mut obs_keys = top_sum;
    for (parent, value) in sub_causes {
        if !top_causes.contains(parent) {
            add(&mut obs_keys, value, "indeterminate_keys")?;
        }
    }
    obs_keys = obs_keys.max(total_counter);
    Ok((obs_keys, any_indet))
}

/// Computes the census over `loaded` for `selected` observers (in plan order).
///
/// For each selected observer:
/// - if `invariants::side(loaded, &o.id)` is `Side::Shard(Status::Ok, _)` and
///   the shard carries `digests`, its `digest.indeterminate.<cause>` and
///   `digest.cause.<cause>.<word>` counters are aggregated. Because
///   `slip_digest` emits both a top-level `digest.indeterminate.<parent>` and
///   dotted `digest.indeterminate.<parent>.<sub>` (such as `pending` and
///   `pending.<area>`), total `indeterminate_keys` sums top-level
///   `digest.indeterminate.<cause>` counters plus any dotted counter whose
///   top-level parent is absent in that shard, taking `max` with
///   `digest.indeterminate` when present.
/// - if `Side::Shard(Status::Ok, _)` has no `digests` (`spec/verify.md` §5.3
///   P3), it is counted as unclean in `not_ok` under `no_digests`.
/// - `Side::Shard(Status::Diagnosed | Status::Rejected, _)` are counted in
///   `unparsed` and `unparsed_by_status` (`diagnosed`, `rejected`) and do not
///   make the census unclean.
/// - `Side::Shard(Status::Failed, _)` is counted in `not_ok` under `failed`,
///   and `NoJob`/`NoShard`/`Absent`/`Other` under `incomplete`.
pub fn summarize(loaded: &Loaded, selected: &[Digest]) -> Result<Census, String> {
    let selected: BTreeSet<&Digest> = selected.iter().collect();
    let mut observers = 0u64;
    let mut ok = 0u64;
    let mut unparsed = 0u64;
    let mut unparsed_by_status: BTreeMap<String, u64> = BTreeMap::new();
    let mut not_ok = 0u64;
    let mut not_ok_by_status: BTreeMap<String, u64> = BTreeMap::new();
    let mut indeterminate_keys = 0u64;
    let mut indeterminate_observers = 0u64;

    // cause -> (keys, observers, paths)
    let mut causes_map: BTreeMap<String, (u64, u64, Vec<Vec<u8>>)> = BTreeMap::new();
    // text -> (keys, observers)
    let mut texts_map: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for o in loaded.plan.observers.iter().filter(|o| selected.contains(&o.id)) {
        observers += 1;
        match side(loaded, &o.id) {
            Side::Shard(Status::Ok, _) => {
                let Some(Some(shard)) = loaded.shards.get(&o.id) else {
                    unreachable!("Side::Shard(Status::Ok, _) guarantees the shard is loaded");
                };
                if shard.digests.is_none() {
                    not_ok += 1;
                    *not_ok_by_status.entry("no_digests".to_owned()).or_default() += 1;
                    continue;
                }
                ok += 1;
                let (obs_keys, any_indet) =
                    tally_ok_shard(shard, &o.main.path, &mut causes_map, &mut texts_map)?;
                if any_indet || obs_keys > 0 {
                    indeterminate_observers += 1;
                    add(&mut indeterminate_keys, obs_keys, "indeterminate_keys")?;
                }
            }
            Side::Shard(Status::Diagnosed, _) => {
                unparsed += 1;
                *unparsed_by_status.entry("diagnosed".to_owned()).or_default() += 1;
            }
            Side::Shard(Status::Rejected, _) => {
                unparsed += 1;
                *unparsed_by_status.entry("rejected".to_owned()).or_default() += 1;
            }
            Side::Shard(Status::Failed, _) => {
                not_ok += 1;
                *not_ok_by_status.entry("failed".to_owned()).or_default() += 1;
            }
            Side::NoJob | Side::NoShard(_) | Side::Absent(_) | Side::Other(_) => {
                not_ok += 1;
                *not_ok_by_status.entry("incomplete".to_owned()).or_default() += 1;
            }
        }
    }

    let mut causes: Vec<CauseRow> = causes_map
        .into_iter()
        .map(|(cause, (keys, observers, mut paths))| {
            paths.sort();
            paths.truncate(SAMPLE_OBSERVERS);
            CauseRow { cause, keys, observers, sample_observers: paths }
        })
        .collect();
    causes.sort_by(|a, b| b.keys.cmp(&a.keys).then_with(|| a.cause.cmp(&b.cause)));

    let mut cause_texts: Vec<CauseTextRow> = texts_map
        .into_iter()
        .map(|(name, (keys, observers))| CauseTextRow { name, keys, observers })
        .collect();
    cause_texts.sort_by(|a, b| b.keys.cmp(&a.keys).then_with(|| a.name.cmp(&b.name)));

    let omitted = (loaded.plan.observers.len() as u64).saturating_sub(observers);

    Ok(Census {
        observers,
        omitted,
        ok,
        unparsed,
        unparsed_by_status,
        not_ok,
        not_ok_by_status,
        indeterminate_keys,
        indeterminate_observers,
        causes,
        cause_texts,
    })
}

/// Renders the human-readable census output (`spec/harness.md` §9.5).
pub fn render(
    corpus: &str,
    plan: &Digest,
    c: &Census,
    run_path: &Path,
    json_path: &Path,
    selection: bool,
) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "census {corpus}");
    let _ = writeln!(o, "plan {plan}");
    let _ = writeln!(o, "observers {}", c.observers);
    if selection {
        let _ = writeln!(o, "omitted {}", c.omitted);
    }
    let _ = writeln!(o, "ok {}", c.ok);
    let _ = writeln!(o, "unparsed {}", c.unparsed);
    for (status, count) in &c.unparsed_by_status {
        let _ = writeln!(o, "  {status} {count}");
    }
    let _ = writeln!(o, "not_ok {}", c.not_ok);
    for (status, count) in &c.not_ok_by_status {
        let _ = writeln!(o, "  {status} {count}");
    }
    let _ = writeln!(o, "indeterminate_keys {}", c.indeterminate_keys);
    let _ = writeln!(o, "indeterminate_observers {}", c.indeterminate_observers);
    let _ = writeln!(o, "causes {}", c.causes.len());
    for row in &c.causes {
        let _ = writeln!(o, "  {}: {} keys in {} observers", row.cause, row.keys, row.observers);
        for p in &row.sample_observers {
            let _ = writeln!(o, "    {}", text(p));
        }
    }
    let _ = writeln!(o, "cause_texts {}", c.cause_texts.len());
    for row in &c.cause_texts {
        let _ = writeln!(o, "  {}: {} keys in {} observers", row.name, row.keys, row.observers);
    }
    let _ = writeln!(o, "run {}", text(run_path.as_os_str().as_encoded_bytes()));
    let _ = writeln!(o, "json {}", text(json_path.as_os_str().as_encoded_bytes()));
    let _ = writeln!(o, "{}", c.last_line(selection));
    o
}

// ---------------------------------------------------------------------------
// JSON record (`<store>/census.json`)

/// Metadata saved into `census.json` and checked by the store-reuse rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub corpus: String,
    pub plan: Digest,
    pub run: String,
    pub tool_slip: String,
    pub tool_kiln: String,
    pub kiln_pin: String,
    pub slip_digest: Digest,
    pub only: Vec<String>,
    pub limit: Option<u64>,
}

fn s(v: &str) -> Value {
    Value::Str(v.to_owned())
}

fn int(v: u64) -> Value {
    Value::Int(i128::from(v))
}

fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

/// Builds the JSON document written to `<store>/census.json`.
pub fn to_json(meta: &Meta, c: &Census) -> Value {
    let unparsed_obj =
        Value::Object(c.unparsed_by_status.iter().map(|(k, &v)| (k.clone(), int(v))).collect());
    let not_ok_obj =
        Value::Object(c.not_ok_by_status.iter().map(|(k, &v)| (k.clone(), int(v))).collect());
    let causes = Value::Array(
        c.causes
            .iter()
            .map(|r| {
                obj(vec![
                    ("cause", s(&r.cause)),
                    ("keys", int(r.keys)),
                    ("observers", int(r.observers)),
                    (
                        "sample_observers",
                        Value::Array(r.sample_observers.iter().map(|p| s(&text(p))).collect()),
                    ),
                ])
            })
            .collect(),
    );
    let cause_texts = Value::Array(
        c.cause_texts
            .iter()
            .map(|r| {
                obj(vec![
                    ("name", s(&r.name)),
                    ("keys", int(r.keys)),
                    ("observers", int(r.observers)),
                ])
            })
            .collect(),
    );
    let selection = obj(vec![
        ("only", Value::Array(meta.only.iter().map(|x| s(x)).collect())),
        ("limit", meta.limit.map_or(Value::Null, int)),
    ]);
    obj(vec![
        ("format", s(FORMAT)),
        ("corpus", s(&meta.corpus)),
        ("plan", s(&meta.plan.to_string())),
        ("run", s(&meta.run)),
        ("tool_slip", s(&meta.tool_slip)),
        ("tool_kiln", s(&meta.tool_kiln)),
        ("kiln_pin", s(&meta.kiln_pin)),
        ("slip_digest", s(&meta.slip_digest.to_string())),
        ("selection", selection),
        ("clean", Value::Bool(c.is_clean())),
        ("observers", int(c.observers)),
        ("omitted", int(c.omitted)),
        ("ok", int(c.ok)),
        ("unparsed", int(c.unparsed)),
        ("unparsed_by_status", unparsed_obj),
        ("not_ok", int(c.not_ok)),
        ("not_ok_by_status", not_ok_obj),
        ("indeterminate_keys", int(c.indeterminate_keys)),
        ("indeterminate_observers", int(c.indeterminate_observers)),
        ("causes", causes),
        ("cause_texts", cause_texts),
    ])
}

/// Reads the reuse metadata from `<store>/census.json`, returning `None` if
/// absent or malformed (when in doubt, parse again).
fn read_meta(path: &Path) -> Option<Meta> {
    let bytes = std::fs::read(path).ok()?;
    let v = json::parse(&bytes).ok()?;
    if v.get("format")?.as_str()? != FORMAT {
        return None;
    }
    let corpus = v.get("corpus")?.as_str()?.to_owned();
    let plan = Digest::from_hex(v.get("plan")?.as_str()?)?;
    let run = v.get("run")?.as_str()?.to_owned();
    let tool_slip = v.get("tool_slip")?.as_str()?.to_owned();
    let tool_kiln = v.get("tool_kiln")?.as_str()?.to_owned();
    let kiln_pin = v.get("kiln_pin")?.as_str()?.to_owned();
    let slip_digest = Digest::from_hex(v.get("slip_digest")?.as_str()?)?;
    let sel = v.get("selection")?;
    let only: Vec<String> = sel
        .get("only")?
        .as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    let limit = match sel.get("limit")? {
        Value::Null => None,
        other => Some(other.as_u64()?),
    };
    Some(Meta { corpus, plan, run, tool_slip, tool_kiln, kiln_pin, slip_digest, only, limit })
}

fn write_json(path: &Path, doc: &Value) -> Result<(), String> {
    let text = json::write_pretty(doc).map_err(|e| format!("census.json: {e}"))?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Execution

/// The result of running a census over a corpus (used by both
/// `cargo xtask census` and `cargo xtask corpus rename`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub plan: Digest,
    pub census: Census,
    pub run_path: PathBuf,
    pub json_path: PathBuf,
    pub reused: bool,
    pub selection: bool,
}

impl Outcome {
    pub fn last_line(&self) -> String {
        self.census.last_line(self.selection)
    }
}

/// `cargo xtask census …`: exit 0 clean, 1 not clean, 2 command error.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    let env = Env {
        corpus_dir: std::env::var_os("SLIP_CORPUS_DIR"),
        target_dir: std::env::var_os("CARGO_TARGET_DIR"),
    };
    let mut stdout = std::io::stdout();
    match run(repo, args, &env, &mut stdout) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Failure::Usage(e)) => {
            eprintln!("xtask census: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask census: {e}");
            ExitCode::from(2)
        }
    }
}

/// Runs `args` (after `census`), writing the report to `out`.
/// `Ok(true)`: clean; `Ok(false)`: not clean.
pub fn run(repo: &Path, args: &[String], env: &Env, out: &mut dyn Write) -> Result<bool, Failure> {
    let (name, opts) = parse_args(args).map_err(Failure::Usage)?;
    let e = Failure::Error;
    let corpora = rows::load(repo).map_err(e)?;
    let c = rows::find(&corpora, &name).map_err(e)?;
    let root = corpus::corpus_root(repo, env.corpus_dir.as_deref()).map_err(e)?;
    let layout = Layout::new(&root, &c.name);
    configured(c, &layout).map_err(e)?;
    let slip = slip_run::find_slip(repo, opts.common.slip.as_deref(), env.target_dir.as_deref())
        .map_err(e)?;
    let outcome = execute(repo, c, &layout, &slip, &opts).map_err(e)?;
    let rendered = render(
        &c.name,
        &outcome.plan,
        &outcome.census,
        &outcome.run_path,
        &outcome.json_path,
        outcome.selection,
    );
    out.write_all(rendered.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|err| Failure::Error(format!("cannot write the output: {err}")))?;
    Ok(outcome.census.is_clean())
}

/// The default store path for `opts` (`spec/harness.md` §9.5):
/// `<corpus dir>/stores/census` for a full-corpus run, or
/// `<corpus dir>/stores/census-<hex16>` when `--only` or `--limit` is given,
/// where `<hex16>` is the first 16 lowercase hex digits of `content_digest`
/// over the compact JSON `{"only":[...],"limit":...}`.
pub fn default_store(layout: &Layout, opts: &invariants::Options) -> PathBuf {
    let stores = layout.dir.join("stores");
    if !opts.selection() {
        return stores.join("census");
    }
    let selection = obj(vec![
        ("only", Value::Array(opts.only.iter().map(|x| s(x)).collect())),
        ("limit", opts.limit.as_ref().map_or(Value::Null, |(_, n)| int(*n))),
    ]);
    let bytes = json::write_compact(&selection)
        .unwrap_or_else(|e| unreachable!("selection JSON is always valid: {e}"));
    let digest = content_digest(bytes.as_bytes()).to_string();
    stores.join(format!("census-{}", &digest[..16]))
}

/// Runs a census for an already-located corpus and `slip` executable
/// (shared with `cargo xtask corpus rename`).
pub fn execute(
    repo: &Path,
    c: &Corpus,
    layout: &Layout,
    slip: &Path,
    opts: &Options,
) -> Result<Outcome, String> {
    let default = default_store(layout, &opts.common);
    let store = opts.store.clone().unwrap_or_else(|| default.clone());
    let json_path = store.join("census.json");
    let tools = versions(slip)?;
    let slip_bytes = std::fs::read(slip).map_err(|e| format!("{}: {e}", slip.display()))?;
    let slip_digest = content_digest(&slip_bytes);
    let kiln_pin = kiln_pin_of(repo, c);

    let (loaded, run_path, reused) = if let Some((loaded, run_path)) =
        try_reuse(layout, slip, &store, &json_path, &tools, &kiln_pin, &slip_digest, opts)?
    {
        (loaded, run_path, true)
    } else {
        let (loaded, run_path) = parse_into(layout, slip, &store, &default, opts)?;
        (loaded, run_path, false)
    };

    let selected = opts.common.select(&loaded.plan);
    let census = summarize(&loaded, &selected)?;
    let run_rel = run_path.strip_prefix(&store).unwrap_or(&run_path).to_string_lossy().into_owned();
    let meta = Meta {
        corpus: c.name.clone(),
        plan: loaded.path.plan,
        run: run_rel,
        tool_slip: tools.0,
        tool_kiln: tools.1,
        kiln_pin,
        slip_digest,
        only: opts.common.only.clone(),
        limit: opts.common.limit.as_ref().map(|(_, n)| *n),
    };
    write_json(&json_path, &to_json(&meta, &census))?;
    Ok(Outcome {
        plan: loaded.path.plan,
        census,
        run_path,
        json_path,
        reused,
        selection: opts.common.selection(),
    })
}

fn configured(c: &Corpus, layout: &Layout) -> Result<(), String> {
    let s = corpus::status(c, layout);
    if s.configured && s.generated && matches!(s.database, Some(Ok(_))) {
        Ok(())
    } else {
        Err(format!(
            "corpus `{}` is not configured: run `cargo xtask corpus configure {}`",
            c.name, c.name
        ))
    }
}

/// The Kiln submodule's commit (`git -C <repo>/kiln rev-parse HEAD`) when
/// present, else the corpus's pinned tag so tests without a git submodule
/// still have a deterministic pin.
fn kiln_pin_of(repo: &Path, c: &Corpus) -> String {
    let kiln_dir = repo.join("kiln");
    if kiln_dir.exists()
        && let Ok(out) =
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&kiln_dir).output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !s.is_empty() {
            return s;
        }
    }
    c.tag.clone()
}

/// Returns `true` only when `path` is confirmed absent on disk
/// (`ErrorKind::NotFound`). Any existing entry — including a regular file,
/// directory, or dangling symlink — and any other error (such as
/// `PermissionDenied`) fails closed (`false`), forcing a fresh parse.
fn negative_absent(path: &[u8]) -> bool {
    let p = Path::new(OsStr::from_bytes(path));
    matches!(std::fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// Returns `true` only when every loaded shard records both its positive
/// `inputs` and its negative inputs (`negatives`, `spec/model.md` §8.6),
/// every recorded positive input file still exists on disk as a regular file
/// with the exact recorded size and content digest, and every recorded
/// negative path is still confirmed absent on disk. When in doubt, returns
/// `false` so the census parses again.
fn inputs_current(loaded: &Loaded) -> bool {
    let mut seen: BTreeMap<&[u8], (u64, Digest)> = BTreeMap::new();
    let mut seen_neg: BTreeSet<&[u8]> = BTreeSet::new();
    for shard in loaded.shards.values().flatten() {
        let Some(inputs) = &shard.inputs else {
            return false;
        };
        let Some(negatives) = &shard.negatives else {
            return false;
        };
        for f in inputs {
            if let Some(&(size, content)) = seen.get(f.path.as_slice()) {
                if size != f.size || content != f.content {
                    return false;
                }
                continue;
            }
            let path = Path::new(OsStr::from_bytes(&f.path));
            let Ok(meta) = std::fs::metadata(path) else {
                return false;
            };
            if !meta.is_file() || meta.len() != f.size {
                return false;
            }
            let Ok(bytes) = std::fs::read(path) else {
                return false;
            };
            if bytes.len() as u64 != f.size || content_digest(&bytes) != f.content {
                return false;
            }
            seen.insert(f.path.as_slice(), (f.size, f.content));
        }
        for neg in negatives {
            if !seen_neg.insert(neg.as_slice()) {
                continue;
            }
            if !negative_absent(neg) {
                return false;
            }
        }
    }
    true
}

/// Whether `parse_into` is allowed to remove an existing non-reusable `store`:
/// only the current run's default store (`<corpus dir>/stores/census` or
/// `<corpus dir>/stores/census-<hex16>`), or a direct child directory of
/// `<corpus dir>/stores/` that contains a regular `census.json` file.
fn can_remove_store(layout: &Layout, store: &Path, default_store: &Path) -> bool {
    let plain = |p: &Path| {
        p.is_absolute()
            && p.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
    };
    if !plain(&layout.dir) || !plain(store) {
        return false;
    }
    let stores_dir = layout.dir.join("stores");
    if !std::fs::symlink_metadata(&stores_dir).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    if store == default_store {
        return true;
    }
    if store.parent() != Some(stores_dir.as_path()) {
        return false;
    }
    let is_real_dir = std::fs::symlink_metadata(store).is_ok_and(|m| m.is_dir());
    let has_census_json =
        std::fs::symlink_metadata(store.join("census.json")).is_ok_and(|m| m.is_file());
    is_real_dir && has_census_json
}

#[allow(clippy::too_many_arguments, reason = "explicit inputs of the store-reuse predicate")]
fn try_reuse(
    layout: &Layout,
    slip: &Path,
    store: &Path,
    json_path: &Path,
    tools: &(String, String),
    kiln_pin: &str,
    slip_digest: &Digest,
    opts: &Options,
) -> Result<Option<(Loaded, PathBuf)>, String> {
    if !store.is_dir() {
        return Ok(None);
    }
    let Some(prev) = read_meta(json_path) else {
        return Ok(None);
    };
    if prev.slip_digest != *slip_digest
        || prev.kiln_pin != kiln_pin
        || prev.tool_slip != tools.0
        || prev.tool_kiln != tools.1
        || prev.only != opts.common.only
        || prev.limit != opts.common.limit.as_ref().map(|(_, n)| *n)
    {
        return Ok(None);
    }
    let args = vec![
        "plan".into(),
        "--db".into(),
        layout.database().into(),
        "--store".into(),
        store.into(),
    ];
    let ran = run_slip(slip, &args, &[0, 1])?;
    let plan = slip_run::line(&ran.stdout, "plan")
        .and_then(Digest::from_hex)
        .ok_or_else(|| "`slip plan` printed no `plan <id>` line".to_owned())?;
    if plan != prev.plan {
        return Ok(None);
    }
    let Some(run_path) = latest_map_run(store, &plan)? else {
        return Ok(None);
    };
    let Some(run_rel) = run_path.strip_prefix(store).ok().map(|p| p.to_string_lossy()) else {
        return Ok(None);
    };
    if run_rel != prev.run {
        return Ok(None);
    }
    let Ok(loaded) = perf::load(&run_path) else {
        return Ok(None);
    };
    let run = &loaded.record.run;
    let selected: BTreeSet<Digest> = opts.common.select(&loaded.plan).into_iter().collect();
    let observers: BTreeSet<Digest> = loaded.shards.keys().copied().collect();
    let same = (run.tool_slip.as_str(), run.tool_kiln.as_str()) == (&tools.0[..], &tools.1[..])
        && run.round == 0
        && loaded.missing_shards() == 0
        && observers == selected
        && inputs_current(&loaded);
    Ok(same.then_some((loaded, run_path)))
}

fn parse_into(
    layout: &Layout,
    slip: &Path,
    store: &Path,
    default_store: &Path,
    opts: &Options,
) -> Result<(Loaded, PathBuf), String> {
    if std::fs::symlink_metadata(store).is_ok() {
        if can_remove_store(layout, store, default_store) {
            slip_run::remove_tree(&layout.dir, store)?;
        } else {
            let stores_dir = layout.dir.join("stores");
            return Err(format!(
                "{} exists and cannot be reused; refusing to remove it (only {} or a direct \
                 child of {} containing census.json is removed automatically); remove it or pass \
                 a fresh `--store`",
                store.display(),
                default_store.display(),
                stores_dir.display(),
            ));
        }
    }
    if let Some(parent) = store.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let mut args: Vec<OsString> = vec![
        "parse".into(),
        "--db".into(),
        layout.database().into(),
        "--store".into(),
        store.into(),
    ];
    for o in &opts.common.only {
        args.extend(["--only".into(), o.into()]);
    }
    if let Some((l, _)) = &opts.common.limit {
        args.extend(["--limit".into(), l.into()]);
    }
    if let Some(j) = &opts.common.jobs {
        args.extend(["-j".into(), j.into()]);
    }
    let ran = run_slip(slip, &args, &[0, 1])?;
    let plan = slip_run::line(&ran.stdout, "plan")
        .and_then(Digest::from_hex)
        .ok_or_else(|| "`slip parse` printed no `plan <id>` line".to_owned())?;
    let run_path = latest_map_run(store, &plan)?.ok_or_else(|| {
        format!("{}: `slip parse` left no map run of plan {plan}", store.display())
    })?;
    let loaded = perf::load(&run_path)?;
    Ok((loaded, run_path))
}
