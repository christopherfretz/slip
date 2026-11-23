//! Reduce (`spec/edits.md` §5): a deterministic function from the map
//! shards of a transform run to one edit set (§6) and its declared changes
//! (§6.2). Works on model types only; no Kiln.
//!
//! Two passes over the shards. The first (§5.1 to §5.3) checks each shard,
//! joins its subjects, links, rules and group ties in memory and collapses
//! its claims, reports, `changed` entries, failures and files into runs of
//! at most `fan_in` shards, each a temporary file sorted by site; the runs
//! are merged `fan_in` at a time until one remains (§5.6). Streaming that
//! one file, each site goes through §5.2's treatment and §5.4's table;
//! only the live sites stay in memory. The second pass, when there is a
//! live site, reads the shards again for file agreement (§5.4). Then the
//! groups and the effect (§5.5), and the set's records (§6).
//! Decisions the spec left open: `spec/landed/edits/WP-062.md`.

#![forbid(unsafe_code)]

mod declared;
mod geometry;
mod groups;
mod outcome;
mod pass1;
mod records;
mod runs;
mod set;
mod subjects;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use slip_model::Digest;
use slip_model::declared::Declared;
use slip_model::editset::EditSet;
use slip_model::hash::{BASIS_TAG, Hasher};
use slip_model::keys::Rule;
use slip_model::transform::{Transform, TransformKind};
use slip_model::wire::Encoder;

pub use crate::declared::declared;

use crate::pass1::{Ctx, Joined};
use crate::records::{Key, Sample, Val};
use crate::runs::{BasisEntry, BasisReader, Cleanup, RunReader, Scratch};
use crate::set::{Builder, Context, Parts};

/// How reduce runs. Nothing here changes the edit set (§5.3).
#[derive(Clone, Debug)]
pub struct Options {
    /// The most shards of one run, and the most temporary files one merge
    /// reads. At least 2; §5.6's default is 64.
    pub fan_in: usize,
    /// The most threads of the first pass and of each merge level. At
    /// least 1.
    pub threads: usize,
    /// The directory of the temporary files (`tmp/` of the store). Reduce
    /// removes every file it made there before it returns.
    pub scratch: PathBuf,
}

impl Options {
    /// §5.6's `fan_in` of 64, one thread, temporary files under `scratch`.
    pub fn new(scratch: PathBuf) -> Options {
        Options { fan_in: 64, threads: 1, scratch }
    }
}

/// The shards of a run, in plan order (§5.7). `Sync`: the first pass and
/// the second read shards from several threads (landed WP-062 §1).
#[allow(clippy::len_without_is_empty)]
pub trait Shards: Sync {
    /// The bytes of the stored shard of the observer at `index` of the
    /// plan, or `None` (unparsed). An `Err` is an `Input` error naming the
    /// observer. Reduce decodes them, and their trailer is the `shard` of
    /// the `Basis` (§6): the id is of the bytes read, never asked for apart
    /// from them.
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String>;
    /// The number of observers of the plan.
    fn len(&self) -> usize;
    /// The observer at `index` of the plan.
    fn observer(&self, index: usize) -> Digest;
}

/// What reduce made: the edit set, and its declared changes if it was
/// accepted (§6.2).
#[derive(Clone, Debug)]
pub struct Reduced {
    pub edits: EditSet,
    pub declared: Option<Declared>,
}

/// Why reduce made no edit set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReduceError {
    /// A shard breaks §5.1 or §4.7, or two shards disagree about a key's
    /// rule (§5.2 rule 1): the least observer index that shows it.
    Input { observer: Digest, message: String },
    /// The temporary files could not be written or read, or reduce made a
    /// set its own reader refuses.
    Io(String),
    /// The [`Options`] are not usable: `fan_in` below 2 or `threads` 0.
    Options(String),
}

impl std::fmt::Display for ReduceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReduceError::Input { observer, message } => {
                write!(f, "the shard of observer {observer}: {message}")
            }
            ReduceError::Io(m) => write!(f, "reduce: {m}"),
            ReduceError::Options(m) => write!(f, "reduce: the options: {m}"),
        }
    }
}

impl std::error::Error for ReduceError {}

/// The least index of an error seen so far, and its message.
struct Least(Mutex<Option<(usize, String)>>);

impl Least {
    fn new() -> Least {
        Least(Mutex::new(None))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(usize, String)>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether an error at a lower index is known: `index` cannot be the
    /// least.
    fn beaten(&self, index: usize) -> bool {
        self.lock().as_ref().is_some_and(|(i, _)| *i < index)
    }

    fn offer(&self, index: usize, message: String) {
        let mut g = self.lock();
        if g.as_ref().is_none_or(|(i, _)| index < *i) {
            *g = Some((index, message));
        }
    }

    fn take(self) -> Option<(usize, String)> {
        self.0.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Runs `job(0..count)` on up to `threads` threads, each taking the next
/// index. Stops handing out work after the first `Err` and returns it.
/// Nothing is kept per job: a job folds its result into shared state, so
/// that memory does not grow with the number of runs (§5.6).
fn pool(
    threads: usize,
    count: usize,
    job: &(dyn Fn(usize) -> Result<(), String> + Sync),
) -> Result<(), String> {
    let next = AtomicUsize::new(0);
    let failed = Mutex::new(None::<String>);
    let work = || {
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= count {
                break;
            }
            match job(i) {
                Ok(()) => {}
                Err(e) => {
                    next.store(count, Ordering::Relaxed);
                    let mut f = failed.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    f.get_or_insert(e);
                    break;
                }
            }
        }
    };
    let n = threads.min(count).max(1);
    if n == 1 {
        work();
    } else {
        std::thread::scope(|s| {
            for _ in 0..n {
                s.spawn(work);
            }
        });
    }
    match failed.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// A run of the first pass: its shards' joined state.
fn first_pass_run(
    ctx: &Ctx<'_>,
    scratch: &Scratch,
    fan_in: usize,
    run: usize,
    least: &Least,
) -> Result<Joined, String> {
    let n = ctx.shards.len();
    let start = run * fan_in;
    let end = (start + fan_in).min(n);
    let mut records: BTreeMap<Key, Val> = BTreeMap::new();
    let mut joined = Joined::default();
    let mut entries = Vec::with_capacity(end - start);
    for index in start..end {
        // Every index, read or not: the merge counts the indices of each
        // observer (§5.7: one observer once in a plan).
        let one = Val::Sample(Sample::one(index as u64));
        match records.entry(Key::Observer(ctx.shards.observer(index))) {
            std::collections::btree_map::Entry::Occupied(mut e) => e.get_mut().join(&one)?,
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(one);
            }
        }
        if least.beaten(index) {
            entries.push(BasisEntry { observer: ctx.shards.observer(index), shard: None });
            continue;
        }
        match pass1::read(ctx, index) {
            Err(m) => {
                least.offer(index, m);
                entries.push(BasisEntry { observer: ctx.shards.observer(index), shard: None });
            }
            Ok(r) => {
                if let Some(shard) = &r.shard
                    && let Err(m) = pass1::fold(ctx, index as u64, shard, &mut records, &mut joined)
                {
                    least.offer(index, m);
                }
                entries.push(r.entry);
            }
        }
    }
    runs::write_basis(&scratch.basis(), start, &entries)?;
    runs::write_run(scratch.run(0, run), &records)?;
    Ok(joined)
}

/// The second pass (§5.4's file agreement) over one run of shards: every
/// `(size, content)` an `ok` shard gives a file of `wanted` in `inputs` or
/// `files`. Also checks that the shard is the one the first pass read.
fn second_pass_run(
    shards: &dyn Shards,
    scratch: &Scratch,
    fan_in: usize,
    run: usize,
    wanted: &BTreeMap<&[u8], u32>,
    least: &Least,
) -> Result<BTreeMap<u32, BTreeSet<(u64, Digest)>>, String> {
    let n = shards.len();
    let start = run * fan_in;
    let end = (start + fan_in).min(n);
    let mut seen: BTreeMap<u32, BTreeSet<(u64, Digest)>> = BTreeMap::new();
    let mut basis = BasisReader::open(&scratch.basis(), start)?;
    for index in start..end {
        let entry = basis.next()?;
        let Some(id) = entry.shard else { continue };
        if least.beaten(index) {
            continue;
        }
        let changed = "the shard changed between the two passes of reduce";
        // The id of what this pass read against the id of what the first
        // pass read: bytes that no longer decode changed too.
        let shard = match shards.open(index) {
            Ok(Some(bytes)) => match pass1::decode(&bytes) {
                Ok((s, again)) if again == id => s,
                _ => {
                    least.offer(index, changed.into());
                    continue;
                }
            },
            Ok(None) => {
                least.offer(index, changed.into());
                continue;
            }
            Err(e) => {
                least.offer(index, format!("the shard cannot be read: {e}"));
                continue;
            }
        };
        let mut add = |path: &[u8], size: u64, content: Digest| {
            if let Some(&pid) = wanted.get(path) {
                seen.entry(pid).or_default().insert((size, content));
            }
        };
        for f in shard.inputs.iter().flatten() {
            add(&f.path, f.size, f.content);
        }
        for f in shard.edits.iter().flat_map(|e| e.files.iter()) {
            add(&f.path, f.size, f.content);
        }
    }
    Ok(seen)
}

/// The basis of the set (§6) and its unparsed observers, from the basis
/// file, in plan order.
fn basis_of(scratch: &Scratch, n: usize) -> Result<(Digest, Vec<Digest>), String> {
    let mut hasher = Hasher::new();
    hasher.update(BASIS_TAG.as_bytes()).update(&[0]);
    let mut enc = Encoder::new();
    enc.count(n);
    hasher.update(enc.as_bytes());
    let mut unparsed = Vec::new();
    let mut reader = BasisReader::open(&scratch.basis(), 0)?;
    for _ in 0..n {
        let b = reader.next()?;
        let mut enc = Encoder::new();
        enc.digest(&b.observer);
        match &b.shard {
            None => {
                enc.u8(0);
                unparsed.push(b.observer);
            }
            Some(s) => {
                enc.u8(1).digest(s);
            }
        }
        hasher.update(enc.as_bytes());
    }
    Ok((hasher.finalize(), unparsed))
}

/// Reduces the shards of round `round` of plan `plan` for the transform
/// `request` (its id `request_id`) to one edit set (§5, §6).
///
/// # Errors
///
/// [`ReduceError::Input`] for a shard that breaks §5.1 or §4.7, or a plan
/// that names one observer twice, naming the least observer index that
/// shows an error; [`ReduceError::Io`] if the temporary files fail;
/// [`ReduceError::Options`] if `options.fan_in` is less than 2 or
/// `options.threads` is 0.
pub fn reduce(
    request: &Transform,
    request_id: Digest,
    plan: Digest,
    round: u64,
    shards: &dyn Shards,
    options: &Options,
) -> Result<Reduced, ReduceError> {
    if options.fan_in < 2 {
        return Err(ReduceError::Options(format!("fan_in is {}, not at least 2", options.fan_in)));
    }
    if options.threads == 0 {
        return Err(ReduceError::Options("threads is 0, not at least 1".into()));
    }
    let scratch = Scratch::new(&options.scratch);
    let mut cleanup = Cleanup::new(&scratch);
    let r = run(request, request_id, plan, round, shards, options, &scratch, &mut cleanup);
    drop(cleanup);
    r
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run(
    request: &Transform,
    request_id: Digest,
    plan: Digest,
    round: u64,
    shards: &dyn Shards,
    options: &Options,
    scratch: &Scratch,
    cleanup: &mut Cleanup<'_>,
) -> Result<Reduced, ReduceError> {
    let fan_in = options.fan_in;
    let n = shards.len();
    let input = |index: usize, message: String| ReduceError::Input {
        observer: shards.observer(index),
        message,
    };

    // Pass 1.
    let ctx = Ctx { request, request_id, round, shards };
    let n_runs = n.div_ceil(fan_in).max(1);
    cleanup.basis = true;
    runs::create_basis(&scratch.basis(), n).map_err(ReduceError::Io)?;
    cleanup.levels.push(n_runs);
    let least = Least::new();
    // The runs' states are joined in run order as they finish: only the
    // runs that finish before an earlier one are held (§5.6).
    let folding = Mutex::new((0usize, BTreeMap::<usize, Joined>::new(), Joined::default()));
    pool(options.threads, n_runs, &|r| {
        let part = first_pass_run(&ctx, scratch, fan_in, r, &least)?;
        let mut f = folding.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (next, waiting, joined) = &mut *f;
        waiting.insert(r, part);
        while let Some(p) = waiting.remove(next) {
            joined.join(p);
            *next += 1;
        }
        Ok(())
    })
    .map_err(ReduceError::Io)?;
    let mut joined = folding.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner).2;
    // Merge.
    let mut level = 0;
    let mut count = n_runs;
    while count > 1 {
        let next = count.div_ceil(fan_in);
        cleanup.levels.push(next);
        let lv = level;
        pool(options.threads, next, &|g| {
            let inputs: Vec<PathBuf> =
                (g * fan_in..((g + 1) * fan_in).min(count)).map(|i| scratch.run(lv, i)).collect();
            runs::merge(&inputs, scratch.run(lv + 1, g))?;
            for p in &inputs {
                std::fs::remove_file(p).map_err(|e| runs::io(p, &e))?;
            }
            Ok(())
        })
        .map_err(ReduceError::Io)?;
        level += 1;
        count = next;
    }

    let mut error = least.take();
    if let Some((c, id)) = joined.rule_conflict() {
        let c = usize::try_from(c).unwrap_or(usize::MAX);
        if error.as_ref().is_none_or(|(i, _)| c < *i) {
            error = Some((
                c,
                format!("the subject {id} has a rule other than an earlier shard gives it"),
            ));
        }
    }

    // The merged stream opens with the observer records (they sort first):
    // a plan that names one observer twice is an error at the second index
    // that names it, the least with every error of the first pass (§5.7).
    let mut reader = RunReader::open(scratch.run(level, 0)).map_err(ReduceError::Io)?;
    let mut first = None;
    while let Some((k, v)) = reader.next().map_err(ReduceError::Io)? {
        let (Key::Observer(o), Val::Sample(s)) = (&k, &v) else {
            first = Some((k, v));
            break;
        };
        if let [a, b, ..] = s.first[..] {
            let i = usize::try_from(b).unwrap_or(usize::MAX);
            if error.as_ref().is_none_or(|(e, _)| i < *e) {
                error = Some((
                    i,
                    format!("the plan names the observer {o} at index {a} and again here"),
                ));
            }
        }
    }
    if let Some((i, m)) = error {
        return Err(input(i, m));
    }

    // Selection.
    let rename = request.kind == TransformKind::Rename;
    let selection = subjects::select(&joined.subjects, &joined.links, rename);
    let refused_subjects: BTreeSet<Digest> =
        selection.refusals.iter().filter_map(|r| r.subject).collect();

    // The stream.
    let mut builder = Builder::default();
    {
        let mut cx = Context { selected: &selection.selected, ties: &mut joined.ties };
        if let Some((k, v)) = first {
            builder.push(k, v, &mut cx).map_err(ReduceError::Io)?;
            while let Some((k, v)) = reader.next().map_err(ReduceError::Io)? {
                builder.push(k, v, &mut cx).map_err(ReduceError::Io)?;
            }
        }
        builder.finish(&mut cx).map_err(ReduceError::Io)?;
    }
    builder.geometry();

    // Pass 2.
    let files = builder.agreement_files();
    let mut seen: BTreeMap<u32, BTreeSet<(u64, Digest)>> = BTreeMap::new();
    if !files.is_empty() {
        let wanted: BTreeMap<&[u8], u32> =
            files.iter().map(|&pid| (builder.paths[pid as usize].as_slice(), pid)).collect();
        let n_runs = n.div_ceil(fan_in);
        let least = Least::new();
        // A union: the order in which runs finish does not matter.
        let joined_seen = Mutex::new(BTreeMap::<u32, BTreeSet<(u64, Digest)>>::new());
        pool(options.threads, n_runs, &|r| {
            let part = second_pass_run(shards, scratch, fan_in, r, &wanted, &least)?;
            let mut all = joined_seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            for (pid, s) in part {
                all.entry(pid).or_default().extend(s);
            }
            Ok(())
        })
        .map_err(ReduceError::Io)?;
        if let Some((i, m)) = least.take() {
            return Err(input(i, m));
        }
        seen = joined_seen.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    builder.file_agreement(&seen);
    builder.groups(request.indivisible, &refused_subjects, !selection.refusals.is_empty());

    // The set.
    let (basis, unparsed) = basis_of(scratch, n).map_err(ReduceError::Io)?;
    let set_subjects: Vec<_> = joined
        .subjects
        .iter()
        .filter(|(id, _)| selection.selected.contains(*id))
        .map(|(id, s)| s.to_subject(*id))
        .collect();
    let union: BTreeSet<Rule> = joined
        .rules
        .iter()
        .cloned()
        .chain(set_subjects.iter().filter_map(|s| s.rule.clone()))
        .collect();
    let rule_conflict = declared::conflicts(&union);
    let stats = &joined.stats;
    let mut counters = joined.counters.clone();
    let mut put = |name: &str, v: u64| {
        counters.insert(name.to_owned(), v);
    };
    put("reduce.shards", stats.ok);
    put("reduce.unparsed", unparsed.len() as u64);
    put("reduce.failed_runs", stats.failed_runs);
    put("reduce.claims", stats.claims);
    for (i, name) in ["replace", "keep", "refuse", "unresolved"].iter().enumerate() {
        put(&format!("reduce.claims.{name}"), stats.by_stance[i]);
    }
    put("reduce.subjects", joined.subjects.len() as u64);
    put("reduce.subjects.selected", set_subjects.len() as u64);
    put("reduce.rules", union.len() as u64);
    let observer = |i: u64| shards.observer(usize::try_from(i).unwrap_or(usize::MAX));
    let set = builder.assemble(Parts {
        plan,
        round,
        transform: request_id,
        basis,
        indivisible: request.indivisible,
        subjects: set_subjects,
        rules: declared::table_of(&joined.rules),
        subject_refusals: selection.refusals,
        no_observer: stats.ok == stats.failed_runs,
        rule_conflict,
        unparsed,
        counters,
        observer: &observer,
    });
    set.validate()
        .map_err(|e| ReduceError::Io(format!("reduce made an edit set its reader refuses: {e}")))?;
    let declared = declared(&set).map_err(ReduceError::Io)?;
    Ok(Reduced { edits: set, declared })
}
