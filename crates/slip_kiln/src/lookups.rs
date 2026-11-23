//! What a unit's header search looked at, from Kiln's lookup log
//! (Kiln `kiln_pp/src/lookups.rs`, Kiln `spec/pp.md` "Lookup log"), in
//! Slip's own terms: [`Lookups`]. Asked for by
//! [`crate::ParseOptions::lookups`]; read by the verify task's guard L1
//! (`spec/verify.md` §5.3, work package WP-052) and by the map task for
//! the shard's negative inputs (`spec/model.md` §8.6, work package WP-117).

use std::collections::HashSet;

use kiln_driver::front_end::FrontEnd;
use kiln_pp::{DirGroup, Found, LookupForm, LookupLog, join_path};

use crate::kiln::source::FileOrigin;

/// The places one unit's front end looked at for a file, as the front end
/// spelled them: a relative one is relative to the command's `directory`.
///
/// Every directory here is one the front end opened a name in, or would
/// have, for a name it was asked for: what it found there and what it did
/// not find. What is outside this list is listed in `spec/verify.md` §5.3
/// ("What L1 does not see").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lookups {
    /// The search list's directories, in order (Kiln `LookupLog::list`).
    pub list: Vec<String>,
    /// Each location a recorded lookup tried, `(directory, name)`: the file
    /// `directory/name` was opened or looked for. Unique, in the order first
    /// seen. For a `#include` form: the includer's directory where the form
    /// tries it, then the search list from where the form starts or
    /// continues, through the directory it was found in (all of it when it
    /// was not found). For an `#embed` form: the includer's directory and
    /// `.` for `"..."`, then every `--embed-dir` (the log does not say which
    /// of them were tried).
    pub tried: Vec<(String, String)>,
    /// The **negative inputs** (`spec/model.md` §8.6): each location
    /// `(directory, name)` where a recorded lookup looked for `name` and
    /// found nothing, unique, in the order first seen. That is `tried` less
    /// the place each lookup found its file at, with two differences: an
    /// `#embed` form lists the places before the one it read (its order is
    /// the includer's directory, `.`, then each `--embed-dir`), not every
    /// `--embed-dir`; and a search directory of the command (`-I`,
    /// `-iquote`, `-isystem`, `-idirafter`, `-cxx-isystem`) that Kiln's
    /// driver left out of the search list because it was no directory is a
    /// place for every name a `#include` form looked for in the list
    /// (except a `<...>` name in an `-iquote` directory): were it created,
    /// the search would look there, wherever it stands in the list.
    pub missed: Vec<(String, String)>,
    /// The name of every buffer read from disk and of every consulted path
    /// (each of the shard's `inputs`, as spelled when it was read).
    pub read: Vec<String>,
    /// The file name of every `#line` directive and line marker that has
    /// one, in the buffers' order then the directives' (Kiln
    /// `SourceManager::line_directives`): text of the source, which Kiln's
    /// prefix maps apply to as to a path.
    pub line_names: Vec<String>,
}

/// What [`Lookups::of`] takes from Kiln's driver: what the lookup log does
/// not name.
#[derive(Clone, Debug, Default)]
pub(crate) struct Driver {
    /// `--embed-dir` directories, in order (`Args::embed_dirs`).
    pub(crate) embed_dirs: Vec<String>,
    /// The command's own search directories, in order, as the search list
    /// names them (`Args::dirs`, `-I=` made sysroot-relative), each with
    /// whether it is an `-iquote` directory.
    pub(crate) dirs: Vec<(bool, String)>,
}

impl Driver {
    /// From the driver's arguments (`kiln_driver::pp_cmd::Job::search_specs`
    /// for the command's own directories).
    pub(crate) fn of(args: &kiln_driver::args::Args) -> Driver {
        let sysroot = args.isysroot.as_deref().or(args.sysroot.as_deref()).unwrap_or("");
        let dirs = args
            .dirs
            .iter()
            .map(|(g, d)| {
                let path = match d.strip_prefix('=') {
                    Some(rest) if !sysroot.is_empty() => format!("{sysroot}{rest}"),
                    _ => d.clone(),
                };
                (*g == DirGroup::Quoted, path)
            })
            .collect();
        Driver { embed_dirs: args.embed_dirs.clone(), dirs }
    }
}

/// A set of `(directory, name)` places, in the order first seen.
#[derive(Default)]
struct Places {
    out: Vec<(String, String)>,
    seen: HashSet<(String, String)>,
}

impl Places {
    fn push(&mut self, d: &str, name: &str) {
        let key = (d.to_owned(), name.to_owned());
        if !self.seen.contains(&key) {
            self.seen.insert(key.clone());
            self.out.push(key);
        }
    }
}

impl Lookups {
    /// The lookups of `fe`, whose log Kiln recorded (`None` without one).
    /// `driver` is called once, for what the log does not name.
    pub(crate) fn of(fe: &FrontEnd<'_, '_>, driver: impl FnOnce() -> Driver) -> Option<Self> {
        let log = fe.lookups?;
        let driver = driver();
        let list: Vec<String> = log.list.iter().map(|&d| dir(log, d).to_owned()).collect();
        // The command's search directories the driver left out because
        // they are no directory (a duplicate of a listed one is one).
        let listed: HashSet<&str> = list.iter().map(String::as_str).collect();
        let absent: Vec<&(bool, String)> = driver
            .dirs
            .iter()
            .filter(|(_, d)| {
                !listed.contains(d.as_str()) && !std::fs::metadata(d).is_ok_and(|m| m.is_dir())
            })
            .collect();
        let mut tried = Places::default();
        let mut missed = Places::default();
        for l in &log.lookups {
            // An absolute name is tried where it is, whatever the form
            // (Kiln's log does not record one yet: `spec/verify.md` §5.1,
            // "What remains outside").
            if l.name.starts_with('/') {
                tried.push("/", &l.name);
                if matches!(l.found, Found::Missing) {
                    missed.push("/", &l.name);
                }
                continue;
            }
            if l.form == LookupForm::Embed {
                embed(l, log, &driver.embed_dirs, &mut tried, &mut missed);
                continue;
            }
            if let Some(i) = l.includer {
                tried.push(dir(log, i), &l.name);
                if l.found == Found::Includer {
                    continue;
                }
                missed.push(dir(log, i), &l.name);
            } else if l.found == Found::Includer {
                continue;
            }
            let start = match (l.form, l.after) {
                // The first place of `after` in the list (an earlier one
                // only adds directories: the guard sees more, never less).
                (LookupForm::Next, Some(after)) => {
                    log.list.iter().position(|&d| d == after).map_or(0, |p| p + 1)
                }
                (LookupForm::Next, None) => 0,
                _ if l.angled => log.angled_start as usize,
                _ => 0,
            };
            let start = start.min(log.list.len());
            let hit = match l.found {
                Found::Dir { dir: hit, .. } => {
                    log.list[start..].iter().position(|&d| d == hit).map(|p| start + p)
                }
                _ => None,
            };
            let end = hit.map_or(log.list.len(), |h| h + 1);
            for &d in &log.list[start..end] {
                tried.push(dir(log, d), &l.name);
            }
            for &d in &log.list[start..hit.unwrap_or(end)] {
                missed.push(dir(log, d), &l.name);
            }
            for (quoted, d) in &absent {
                if !(l.angled && *quoted) {
                    missed.push(d, &l.name);
                }
            }
        }
        let mut out = Lookups { list, tried: tried.out, missed: missed.out, ..Lookups::default() };
        for file in fe.sm.file_ids() {
            let read = match fe.sm.file_origin(file) {
                FileOrigin::Virtual => false,
                FileOrigin::Disk => true,
                FileOrigin::Lazy => fe.sm.original_bytes(file).is_some(),
            };
            if read {
                out.read.push(fe.sm.file_name(file).to_owned());
            }
            if fe.sm.has_line_directives(file) {
                out.line_names.extend(fe.sm.line_directives(file).into_iter().filter_map(|d| d.2));
            }
        }
        out.read.extend(fe.consulted.iter().map(|c| c.path.clone()));
        Some(out)
    }
}

/// An `#embed` or `__has_embed` lookup `l`: Kiln tries, in order, the
/// includer's directory and `.` for `"..."` (the bare name), then every
/// `--embed-dir` (Kiln `HeaderSearch::lookup_embed`), and reads the first
/// that is a readable regular file. Every one is `tried` (the log does not
/// say where the search stopped); the ones before the path it read are
/// `missed`, all of them when it read none.
fn embed(
    l: &kiln_pp::Lookup,
    log: &LookupLog,
    embed_dirs: &[String],
    tried: &mut Places,
    missed: &mut Places,
) {
    let mut tries: Vec<(&str, String)> = Vec::new();
    if let Some(i) = l.includer {
        tries.push((dir(log, i), join_path(dir(log, i), &l.name)));
    }
    if !l.angled {
        tries.push((".", l.name.clone()));
    }
    tries.extend(embed_dirs.iter().map(|d| (d.as_str(), join_path(d, &l.name))));
    for (d, _) in &tries {
        tried.push(d, &l.name);
    }
    let read = match &l.found {
        Found::Embed { path } => tries.iter().position(|(_, p)| p == path),
        _ => None,
    };
    // A path read that is none of the tries cannot be placed: every try is
    // then missed, which refuses more, never less.
    for (d, _) in &tries[..read.unwrap_or(tries.len())] {
        missed.push(d, &l.name);
    }
}

/// The directory name `i` of `log` (`""` out of range: Kiln's own log is
/// consistent, and an empty name is the working directory, which the guard
/// then checks).
fn dir(log: &LookupLog, i: u32) -> &str {
    log.dirs.get(i as usize).map_or("", String::as_str)
}
