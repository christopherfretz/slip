//! The hidden `slip __worker` (`spec/exec.md` §8.4): `serve` over standard
//! input and output with [`PhaseTask`], which runs a job by its phase: the
//! map task (`spec/model.md` §8.5: the parse task with the semantic
//! digest's walk run on the finished unit) or the verify task
//! (`spec/verify.md` §5, [`crate::verify_job`]).
//!
//! The worker changes its current directory during a parse
//! (`slip_kiln::parse_observer`), so every path it uses is absolute: the
//! job's store path is (§2 "Path"). Its standard output carries protocol
//! messages only: nothing here prints there.

use std::io::Write;
use std::process::ExitCode;

use slip_digest::map::{counters, map_unit, with_input};
use slip_digest::{Detail, Options};
use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, Post, PostImage, ServeOptions, Task, serve};
use slip_kiln::{ParseOptions, parse_observer_with};
use slip_model::declared::Declared;
use slip_model::edits::Edits;
use slip_model::formats::content_id;
use slip_model::keys::RuleTable;
use slip_model::{Counter, Digest, Observer, Phase, Shard, Status, Transform};

use crate::args::WorkerArgs;
use crate::image::Image;
use crate::verify_job::{Params, verify};

pub mod post;
pub mod transform;

/// Opens the job's store shared into `slot`, unless `slot` holds that
/// store already (`spec/exec.md` §8.4: kept for the task's lifetime; a
/// job naming another store replaces it).
fn open_store<'s>(slot: &'s mut Option<Store>, job: &Job) -> Result<&'s Store, String> {
    let path = job.store_path();
    if slot.as_ref().is_none_or(|store| store.root() != path) {
        *slot = None;
        let store = Store::open(path, Access::Shared)
            .map_err(|e| format!("cannot open the store {}: {e}", path.display()))?;
        *slot = Some(store);
    }
    match slot.as_ref() {
        Some(store) => Ok(store),
        None => unreachable!("the store was just opened"),
    }
}

/// The map task (`spec/model.md` §8.5): one observer through Kiln's front
/// end, the digest's walk run on the finished unit, its shard published in
/// the job's store.
///
/// The store is opened shared on the first job and kept for the task's
/// lifetime; a job naming another store replaces it. The parse itself
/// writes no file (`spec/model.md` §8.4).
#[derive(Debug, Default)]
pub struct MapTask {
    store: Option<Store>,
}

impl MapTask {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Task for MapTask {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        if job.phase != Phase::Map {
            return Err(format!(
                "the map task runs phase map only; the job is of phase {}",
                phase_name(job.phase)
            ));
        }
        // spec/exec.md §8.4: a job that does not fit its phase is `Err`,
        // naming the field.
        if job.post.is_some() {
            return Err("a map job with a post".to_owned());
        }
        let store = open_store(&mut self.store, job)?;
        // spec/edits.md §8.1 (WP-074): with a transform, the request is
        // read and checked, and a kind this worker does not run refused,
        // before anything is parsed.
        let transform = match &job.transform {
            None => None,
            Some(id) => {
                let t = transform::read_transform(store, id)?;
                if t.kind == slip_model::transform::TransformKind::Script {
                    return Err(transform::NO_SCRIPT.to_owned());
                }
                Some((t, *id))
            }
        };
        map_shard(&job.observer, job.round, true, transform.as_ref().map(|(t, id)| (t, *id)))
    }
}

/// The transform's section and counters for an `ok` unit of the map job of
/// the request `id`: the transform ran, and its section names the job's
/// request (the rename is given `id`; this checks that a transform did not
/// name another).
///
/// # Errors
///
/// The transform did not run, or its section's `transform` is not `id`: a
/// defect of Slip, which fails the job.
fn checked_edits(
    edits: Option<(Edits, Vec<Counter>)>,
    id: Digest,
) -> Result<(Edits, Vec<Counter>), String> {
    let Some((section, counters)) = edits else {
        return Err("the unit is ok and the transform did not run".to_owned());
    };
    if section.transform != id {
        return Err(format!(
            "the edits name the request {}, not the job's {id}",
            section.transform
        ));
    }
    Ok((section, counters))
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Map => "map",
        Phase::Verify => "verify",
    }
}

/// The shard of phase `map` of one observer (`spec/model.md` §8.5): the
/// parse task's, with the walk run inside its closure on the front end's
/// thread when the unit has no error (rules none, detail `none`, no
/// names). An `ok` shard gets the walk's `digests` and counters; a shard
/// that is not `ok` drops the walk's result. A walk that reported an error
/// (too little stack, a buffer that is not an input) is `Err`: the job
/// fails.
///
/// With `negatives` (the map task's, always) the parse records Kiln's
/// lookup log and a shard with `inputs` gets its negative inputs
/// (`spec/model.md` §8.6, work package WP-117) and its found lookups
/// (§8.8, work package WP-135). Without, it is the parse of before WP-117:
/// what the measurement of the log's cost compares against
/// (`spec/landed/verify/WP-117.md`).
///
/// # Errors
///
/// The parse task's, the walk's (above), and `negatives::record`'s (the
/// tree changed during the parse).
pub fn map_observer(observer: &Observer, round: u64, negatives: bool) -> Result<Shard, String> {
    map_shard(observer, round, negatives, None)
}

/// [`map_observer`], and with `transform` (the request and its id) the map
/// job of a transform (`spec/edits.md` §8.1; work package WP-074): the
/// parse records Kiln's written-reference tables and preprocessing record
/// (`ParseOptions::written`, `pp_record`), the walk runs as without, and
/// then, in the same closure and on a walk that succeeded,
/// [`transform::run_transform`] over the same input. An `ok` shard gets
/// `edits` and the `edits.*` counters beside the walk's; a shard that is
/// not `ok` has no `edits`. Without `transform` this is M1's map task,
/// byte for byte (the options and the closure's work are those of before).
///
/// # Errors
///
/// [`map_observer`]'s; the transform's (an `Err` of the native rename fails
/// the job, §4.6); an `edits` section whose `transform` is not the job's
/// request id.
fn map_shard(
    observer: &Observer,
    round: u64,
    negatives: bool,
    transform: Option<(&Transform, Digest)>,
) -> Result<Shard, String> {
    let rules = RuleTable::empty();
    let options = Options {
        rules: &rules,
        read_only: slip_digest::ReadOnly::EVERY_FILE,
        detail: Detail::None,
        names: false,
        rule_use: false,
    };
    let identity = |path: &[u8]| path.to_vec();
    let mut parse = ParseOptions::default();
    parse.lookups = negatives;
    if transform.is_some() {
        parse.written = true;
        parse.pp_record = true;
    }
    let directory = &observer.command.directory;
    let (mut shard, ran) = parse_observer_with(observer, Phase::Map, round, parse, |unit| {
        let places = unit.lookups().map(|l| {
            (crate::negatives::places(directory, l), crate::negatives::found_places(directory, l))
        });
        let walked = map_unit(unit, &options, &identity);
        // §8.1: the transform is entered once the walk's output is
        // complete, and only after a walk that succeeded.
        let edits = match (transform, &walked) {
            (Some((t, id)), Some(Ok(_))) => Some(with_input(unit, &identity, |input| {
                transform::run_transform(t, id, unit, input)
            })),
            _ => None,
        };
        (walked, places, edits)
    })?;
    let (walked, places, edits) = ran.map_or((None, None, None), |(w, p, e)| (w, p, e));
    // An `Err` of the transform fails the job, whatever the outcome.
    let edits = edits.map(|ran| ran.and_then(|e| e)).transpose()?;
    if negatives
        && let Some(inputs) = &shard.inputs
        && let Some((missed, found)) = places
    {
        shard.found = Some(crate::negatives::record(found, inputs)?);
        shard.negatives = Some(missed);
    }
    let output = match walked {
        Some(Ok(output)) => output,
        Some(Err(e)) => return Err(e),
        None => return Ok(shard),
    };
    if shard.outcome.status == Status::Ok {
        shard.counters.extend(counters(&output));
        if let Some((_, id)) = transform {
            let (section, edit_counters) = checked_edits(edits, id)?;
            shard.counters.extend(edit_counters);
            shard.edits = Some(section);
        }
        shard.counters.sort_by(|a, b| a.name.cmp(&b.name));
        shard.digests = Some(output.digests);
        shard.names = Some(slip_model::names::Names {
            units: output
                .name_strings
                .iter()
                .map(|n| slip_model::names::NamesEntry { key: n.key, names: n.names })
                .collect(),
            notes: Vec::new(),
        });
    }
    Ok(shard)
}

/// The verify task (`spec/verify.md` §5; `spec/exec.md` §8.4): makes a
/// job's [`Params`] from `job.post` and the worker's `--findings` and
/// `--locate`, then runs [`verify`].
///
/// The store is kept as the map task keeps it. The `Declared` (with its
/// `RuleTable`) and the image are kept from one job to the next while the
/// job's `post.declared` and `post.image` are the same.
#[derive(Debug)]
pub struct VerifyTask {
    store: Option<Store>,
    limit: usize,
    locate: usize,
    /// The parameters of the last job, by its `(declared, image)`.
    kept: Option<(Digest, PostImage, Params)>,
}

impl VerifyTask {
    /// A task whose jobs keep `limit` findings and name `locate`.
    pub fn new(limit: u64, locate: u64) -> Self {
        let size = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
        VerifyTask { store: None, limit: size(limit), locate: size(locate), kept: None }
    }
}

/// The one place a verify job's [`Params`] are made (`spec/exec.md` §8.6):
/// from `post`, the job's store and the worker's `limit` and `locate`.
/// `kept` is the previous job's, reused while `post.declared` and
/// `post.image` are the same.
///
/// # Errors
///
/// The `Declared` is not in the store, is not a valid `DECL`, has
/// another content id than `post.declared`, or an entry of its `files`
/// does not exist or is not spelled on disk
/// ([`crate::commands::entries_on_disk`]); the image is refused
/// (`Image::from_post`).
fn make_params(
    store: &Store,
    post: &Post,
    limit: usize,
    locate: usize,
    kept: Option<(Digest, PostImage, Params)>,
) -> Result<(Digest, PostImage, Params), String> {
    if let Some((declared, image, mut params)) = kept
        && declared == post.declared
        && image == post.image
    {
        params.pre = post.pre;
        return Ok((declared, image, params));
    }
    let id = post.declared;
    let path = store.declared_path(&id);
    let bytes = store
        .read_declared(&id)
        .map_err(|e| format!("cannot read the declared changes {id}: {e}"))?
        .ok_or_else(|| {
            format!("the declared changes {id} are not in the store ({})", path.display())
        })?;
    let declared = Declared::from_bytes(&bytes)
        .map_err(|e| format!("{} is not a valid DECL file: {e}", path.display()))?;
    let actual = content_id(&bytes)
        .map_err(|e| format!("{} is not a valid DECL file: {e}", path.display()))?;
    if actual != id {
        return Err(format!("{} has the content id {actual}, not {id}", path.display()));
    }
    // spec/edits.md §2.1 (WP-151): the run's files, as a request's
    // ownership, exist and are spelled on disk; else the job fails.
    if let Some(files) = declared.files() {
        crate::commands::entries_on_disk(files).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let image = Image::from_post(store, &post.image)?;
    let params = Params { pre: post.pre, declared, declared_id: id, image, limit, locate };
    Ok((id, post.image.clone(), params))
}

impl Task for VerifyTask {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        if job.phase != Phase::Verify {
            return Err(format!(
                "the verify task runs phase verify only; the job is of phase {}",
                phase_name(job.phase)
            ));
        }
        // spec/exec.md §5.2, §8.4: a job that does not fit its phase.
        if job.transform.is_some() {
            return Err("a verify job with a transform".to_owned());
        }
        let Some(post) = &job.post else {
            return Err("a verify job without a post".to_owned());
        };
        let store = open_store(&mut self.store, job)?;
        let (declared, image, params) =
            make_params(store, post, self.limit, self.locate, self.kept.take())?;
        let shard = verify(store, job, &params);
        self.kept = Some((declared, image, params));
        shard
    }
}

/// The worker's task (`spec/exec.md` §8.4): a job of phase `map` goes to
/// the map task, one of phase `verify` to the verify task. Each refuses a
/// job that does not fit its phase.
#[derive(Debug)]
pub struct PhaseTask {
    map: MapTask,
    verify: VerifyTask,
}

impl PhaseTask {
    /// `limit` and `locate` are the worker's `--findings` and `--locate`.
    pub fn new(limit: u64, locate: u64) -> Self {
        PhaseTask { map: MapTask::new(), verify: VerifyTask::new(limit, locate) }
    }
}

impl Task for PhaseTask {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        match job.phase {
            Phase::Map => self.map.run(job),
            Phase::Verify => self.verify.run(job),
        }
    }
}

/// `slip __worker`: serves until the input ends, `--worker-jobs` jobs have
/// run, or a job panicked. Status 0 then; 1 if the protocol failed (the
/// coordinator sees a crash).
pub fn main(args: &WorkerArgs) -> ExitCode {
    let options = ServeOptions {
        tool: crate::version_line(),
        stack_bytes: usize::try_from(args.worker_stack).unwrap_or(usize::MAX),
        max_jobs: args.worker_jobs,
    };
    let task = PhaseTask::new(args.findings, args.locate);
    match serve(task, &options, std::io::stdin(), std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "slip __worker: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    //! `spec/model.md` §8.5 "The walk" and "The shard": the map task's
    //! composition of the parse task and the walk.

    use std::path::PathBuf;
    use std::sync::{Mutex, PoisonError};

    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Status};

    use super::{checked_edits, map_observer};

    /// The parse changes the current directory: one at a time.
    static LOCK: Mutex<()> = Mutex::new(());

    /// Writes `files` to a directory of its own and returns the map shard
    /// of `argv` there.
    fn map_in(
        name: &str,
        files: &[(&str, &str)],
        argv: &[&str],
    ) -> Result<slip_model::Shard, String> {
        let _lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let dir: PathBuf =
            std::env::temp_dir().join(format!("slip-cli-map-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        for (rel, text) in files {
            std::fs::write(dir.join(rel), text).unwrap();
        }
        let observer = Observer {
            id: Digest([3; 32]),
            command: Command {
                directory: dir.to_str().unwrap().as_bytes().to_vec(),
                argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
                env: Vec::new(),
            },
            language: Language::Cxx,
            main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        };
        let shard = map_observer(&observer, 2, true);
        let _ = std::fs::remove_dir_all(&dir);
        shard
    }

    fn names(shard: &slip_model::Shard) -> Vec<&str> {
        shard.counters.iter().map(|c| c.name.as_str()).collect()
    }

    // §8.5 "The shard": an `ok` shard has the walk's `digests` and the
    // parse task's counters with the walk's, sorted by name.
    #[test]
    fn s8_5_ok_shard_has_digests_and_counters() {
        let shard =
            map_in("ok", &[("a.cpp", "int f() { return 1; }\n")], &["c++", "a.cpp"]).unwrap();
        shard.validate().unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{:?}", shard.outcome);
        assert_eq!(shard.round, 2);
        let digests = shard.digests.as_ref().expect("digests");
        assert!(digests.groups.iter().all(|g| g.units.is_none()), "detail none");
        let names = names(&shard);
        for n in ["digest.bytes", "digest.units", "inputs.files", "pp.tokens"] {
            assert!(names.contains(&n), "{n} in {names:?}");
        }
        assert!(names.windows(2).all(|w| w[0] < w[1]), "{names:?}");
    }

    // §8.5 "The walk": with errors the walk does not run; the shard is the
    // parse task's (`diagnosed`, no `digests`, no digest counter).
    #[test]
    fn s8_5_walk_skipped_on_a_unit_with_errors() {
        let shard =
            map_in("diag", &[("a.cpp", "int f() { return nope; }\n")], &["c++", "a.cpp"]).unwrap();
        assert_eq!(shard.outcome.status, Status::Diagnosed, "{:?}", shard.outcome);
        assert!(shard.digests.is_none());
        assert_eq!(names(&shard), ["inputs.bytes", "inputs.files", "pp.tokens"]);
    }

    // §8.5 "The shard": a unit with errors is not walked, so a stack too
    // small for the walk does not fail its job: the shard is the parse
    // task's, `diagnosed` (review F2).
    #[test]
    fn s8_5_unit_with_errors_and_too_little_stack_is_diagnosed() {
        let shard = map_in(
            "diag-stack",
            &[("a.cpp", "int f() { return nope; }\n")],
            &["c++", "-fkiln-stack-size=8M", "a.cpp"],
        )
        .unwrap();
        assert_eq!(shard.outcome.status, Status::Diagnosed, "{:?}", shard.outcome);
        assert!(shard.digests.is_none());
    }

    // §8.5 "The stack": a walk that reports an error fails the job.
    #[test]
    fn s8_5_too_little_stack_fails_the_job() {
        let e =
            map_in("stack", &[("a.cpp", "int x;\n")], &["c++", "-fkiln-stack-size=8M", "a.cpp"])
                .unwrap_err();
        assert!(e.starts_with("the front end's thread has "), "{e}");
    }

    // The map task runs phase `map` only: a job of another phase, and a
    // map job with a `post` (spec/exec.md §5.5, §8.4), are `Err` before
    // anything is opened or parsed.
    #[test]
    fn map_task_refuses_a_job_that_does_not_fit() {
        use slip_exec::worker::{Job, Post, PostImage, Task};
        // A store path under a regular file (the test binary): it cannot
        // be opened, nor created.
        let exe = std::env::current_exe().unwrap();
        let store = exe.join("store").into_os_string().into_encoded_bytes();
        let job = Job {
            store,
            plan: Digest([1; 32]),
            phase: slip_model::Phase::Map,
            round: 0,
            observer: Observer {
                id: Digest([3; 32]),
                command: Command { directory: b"/".to_vec(), argv: Vec::new(), env: Vec::new() },
                language: Language::Cxx,
                main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
                cost_hint: 1,
                origins: vec![Origin { configuration: 0, entry: 0 }],
            },
            transform: None,
            post: None,
        };
        let run = |job: &Job| super::MapTask::new().run(job).unwrap_err();
        let verify = Job { phase: slip_model::Phase::Verify, ..job.clone() };
        assert_eq!(run(&verify), "the map task runs phase map only; the job is of phase verify");
        let post = Job {
            post: Some(Post {
                pre: 0,
                declared: Digest([2; 32]),
                image: PostImage::Tree(Vec::new()),
            }),
            ..job.clone()
        };
        assert_eq!(run(&post), "a map job with a post");
        // The store is opened only after these checks: with neither field,
        // the job gets as far as opening it, and fails there. A map job
        // with a transform (WP-074) reads its request from the store, so it
        // too gets as far.
        assert!(run(&job).starts_with("cannot open the store"), "{}", run(&job));
        let transform = Job { transform: Some(Digest([4; 32])), ..job.clone() };
        assert!(run(&transform).starts_with("cannot open the store"), "{}", run(&transform));
    }

    /// WP-074 review (R10): the section of an `ok` unit must name the job's
    /// request; one that names another, or none, fails the job.
    #[test]
    fn checked_edits_names_the_jobs_request() {
        let section = |transform| slip_model::edits::Edits {
            scheme: slip_model::edits::SCHEME,
            transform,
            failure: None,
            files: vec![],
            subjects: vec![],
            links: vec![],
            claims: vec![],
            groups: vec![],
            reports: vec![],
            changed: vec![],
            renamed: vec![],
            instances: vec![],
        };
        let (job, other) = (Digest([5; 32]), Digest([6; 32]));
        assert!(checked_edits(Some((section(job), vec![])), job).is_ok());
        let e = checked_edits(Some((section(other), vec![])), job).unwrap_err();
        assert_eq!(e, format!("the edits name the request {other}, not the job's {job}"));
        let e = checked_edits(None, job).unwrap_err();
        assert_eq!(e, "the unit is ok and the transform did not run");
    }
}
