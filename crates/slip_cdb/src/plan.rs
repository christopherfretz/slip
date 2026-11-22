//! Building the plan (`spec/cdb.md` §7). Work package WP-007.
//!
//! [`build`] reads each database (§2), runs §4–§6 on every entry, merges
//! entries with the same `ObserverId` into one observer, hashes each main
//! file once, and orders the result as `spec/model.md` §7 requires.
//!
//! Hashing the main files is the only parallel part: a fixed list of
//! distinct real paths is shared out over `threads` scoped threads and the
//! results are put back by index, so the plan never depends on the thread
//! count. Each file is read in pieces of [`READ_CHUNK`] bytes through
//! [`Hasher`]; a main file is never held in memory whole.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use slip_model::hash::{Hasher, content_digest};
use slip_model::{
    Command, Configuration, Digest, Excluded, FileRef, Language, Observer, Origin, Plan, Reason,
};

use crate::check::{FrontEndCheck, Rejection};
use crate::database::{Entry, read_json};
use crate::normalise::normalise;

/// The size of the pieces a main file is read in.
pub const READ_CHUNK: usize = 64 * 1024;

/// One compilation database and the configuration name it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// Configuration name: unique and non-empty.
    pub name: String,
    /// The database file.
    pub path: PathBuf,
}

/// The tool versions recorded in the plan (`spec/model.md` §7 section 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub slip: String,
    pub kiln: String,
}

/// Why no plan was produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The sources are not usable as given: no source, or a configuration
    /// name that is empty or given twice.
    Usage(String),
    /// A database cannot be read, is not JSON, or is not a JSON array
    /// (§2). `path` is the database path as given.
    Database { path: PathBuf, detail: String },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Usage(detail) => f.write_str(detail),
            PlanError::Database { path, detail } => write!(f, "{}: {detail}", path.display()),
        }
    }
}

impl std::error::Error for PlanError {}

/// Builds the plan (`spec/cdb.md` §7).
///
/// `threads` bounds the threads that read and hash main files; 0 and 1 both
/// mean the calling thread. The plan is the same for every value.
pub fn build(
    sources: &[Source],
    check: &dyn FrontEndCheck,
    tool: &Tool,
    threads: usize,
) -> Result<Plan, PlanError> {
    check_names(sources)?;

    let mut configurations = Vec::with_capacity(sources.len());
    let mut excluded = Vec::new();
    let mut candidates: BTreeMap<Digest, Candidate> = BTreeMap::new();

    // Steps 1–3: read each database, run §4–§6 on each entry, merge by id.
    for (c, source) in sources.iter().enumerate() {
        let (database, entries) = read_database(source)?;
        configurations.push(Configuration { name: source.name.clone(), database });
        for (e, entry) in entries.into_iter().enumerate() {
            let origin = Origin { configuration: c as u64, entry: e as u64 };
            let entry = match entry {
                Ok(entry) => entry,
                Err(defect) => {
                    excluded.push(exclusion(
                        origin,
                        &defect.file,
                        Reason::MalformedEntry,
                        defect.detail,
                    ));
                    continue;
                }
            };
            match accept(&entry, check) {
                Ok((command, language, main)) => {
                    let id = command.observer_id();
                    // The same command names the same main file and gets
                    // the same answer from the check; the first entry's are
                    // kept (they differ only if the file system changes
                    // while the plan is built).
                    candidates
                        .entry(id)
                        .or_insert_with(|| Candidate {
                            command,
                            language,
                            main,
                            origins: Vec::new(),
                        })
                        .origins
                        .push((origin, entry.file));
                }
                Err((reason, detail)) => {
                    excluded.push(exclusion(origin, &entry.file, reason, detail));
                }
            }
        }
    }

    // Step 4: hash each distinct main file once.
    let mains: Vec<PathBuf> =
        candidates.values().map(|c| c.main.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let hashed = hash_all(&mains, threads);

    let mut observers = Vec::with_capacity(candidates.len());
    for (id, candidate) in candidates {
        let index = mains.binary_search(&candidate.main).unwrap_or_else(|_| {
            unreachable!("every candidate's main file is in the list that was hashed")
        });
        match &hashed[index] {
            Ok((size, content)) => {
                let mut origins: Vec<Origin> = candidate.origins.iter().map(|(o, _)| *o).collect();
                origins.sort_unstable();
                observers.push(Observer {
                    id,
                    command: candidate.command,
                    language: candidate.language,
                    main: FileRef {
                        path: path_bytes(&candidate.main),
                        size: *size,
                        content: *content,
                    },
                    cost_hint: *size,
                    origins,
                });
            }
            Err(error) => {
                let detail =
                    format!("main file `{}` cannot be read: {error}", candidate.main.display());
                for (origin, file) in &candidate.origins {
                    excluded.push(exclusion(*origin, file, Reason::MissingFile, detail.clone()));
                }
            }
        }
    }

    // Step 5: order.
    observers.sort_unstable_by(|a, b| b.cost_hint.cmp(&a.cost_hint).then(a.id.cmp(&b.id)));
    excluded.sort_unstable_by_key(|x| (x.configuration, x.entry));

    let plan = Plan {
        tool_slip: tool.slip.clone(),
        tool_kiln: tool.kiln.clone(),
        configurations,
        observers,
        excluded,
    };
    debug_assert!(plan.validate().is_ok(), "build made an invalid plan: {:?}", plan.validate());
    Ok(plan)
}

/// An observer before its main file is hashed.
struct Candidate {
    command: Command,
    language: Language,
    /// Real path of the main file.
    main: PathBuf,
    /// Each origin with its entry's `file` as spelled (needed if the main
    /// file cannot be read and every origin becomes an exclusion).
    origins: Vec<(Origin, String)>,
}

/// §7: configuration names are unique and non-empty. At least one source is
/// required.
fn check_names(sources: &[Source]) -> Result<(), PlanError> {
    if sources.is_empty() {
        return Err(PlanError::Usage("no compilation database is given".into()));
    }
    let mut seen = HashSet::with_capacity(sources.len());
    for (i, source) in sources.iter().enumerate() {
        if source.name.is_empty() {
            return Err(PlanError::Usage(format!(
                "configuration {i} (`{}`) has an empty name",
                source.path.display()
            )));
        }
        if !seen.insert(source.name.as_str()) {
            return Err(PlanError::Usage(format!(
                "configuration name `{}` is given more than once",
                source.name
            )));
        }
    }
    Ok(())
}

/// §7 step 1: the database's `FileRef` and its entries.
type Entries = Vec<Result<Entry, crate::database::Defect>>;
fn read_database(source: &Source) -> Result<(FileRef, Entries), PlanError> {
    let fail = |detail: String| PlanError::Database { path: source.path.clone(), detail };
    let bytes = std::fs::read(&source.path).map_err(|e| fail(format!("cannot be read: {e}")))?;
    let real = std::fs::canonicalize(&source.path)
        .map_err(|e| fail(format!("its real path cannot be resolved: {e}")))?;
    let entries = read_json(&bytes).map_err(|e| fail(e.detail))?;
    let database = FileRef {
        path: path_bytes(&real),
        size: bytes.len() as u64,
        content: content_digest(&bytes),
    };
    Ok((database, entries))
}

/// §7 step 2 for a well-formed entry: §4 and §5, then §6.
fn accept(
    entry: &Entry,
    check: &dyn FrontEndCheck,
) -> Result<(Command, Language, PathBuf), (Reason, String)> {
    let normalised = normalise(&entry.directory, &entry.arguments, &entry.file)
        .map_err(|x| (x.reason, x.detail))?;
    let language =
        check.check(&normalised.command, &path_bytes(&normalised.main)).map_err(|r| match r {
            Rejection::UnsupportedLanguage(detail) => (Reason::UnsupportedLanguage, detail),
            Rejection::UnsupportedFlag(detail) => (Reason::UnsupportedFlag, detail),
        })?;
    Ok((normalised.command, language, normalised.main))
}

fn exclusion(origin: Origin, file: &str, reason: Reason, detail: String) -> Excluded {
    Excluded {
        configuration: origin.configuration,
        entry: origin.entry,
        file: file.as_bytes().to_vec(),
        reason,
        detail,
    }
}

/// A path's bytes as the operating system holds them.
fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

/// The size and content digest of each file, or why it cannot be read, in
/// the order of `paths`, whatever `threads` is.
fn hash_all(paths: &[PathBuf], threads: usize) -> Vec<Result<(u64, Digest), String>> {
    let workers = threads.min(paths.len());
    if workers <= 1 {
        return paths.iter().map(|p| hash_file(p).map_err(|e| e.to_string())).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<Option<Result<(u64, Digest), String>>> = vec![None; paths.len()];
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = paths.get(i) else { break };
                        done.push((i, hash_file(path).map_err(|e| e.to_string())));
                    }
                    done
                })
            })
            .collect();
        for handle in handles {
            let done = handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            for (i, result) in done {
                results[i] = Some(result);
            }
        }
    });
    results
        .into_iter()
        .map(|r| r.unwrap_or_else(|| unreachable!("every index below the length is claimed once")))
        .collect()
}

/// Reads a file in pieces, returning the number of bytes read and their
/// content digest.
fn hash_file(path: &Path) -> io::Result<(u64, Digest)> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; READ_CHUNK];
    let mut size = 0u64;
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buffer[..n]);
                size += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok((size, hasher.finalize()))
}
