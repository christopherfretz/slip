//! What a unit's header search looked at, from Kiln's lookup log
//! (Kiln `kiln_pp/src/lookups.rs`, Kiln `spec/pp.md` "Lookup log"), in
//! Slip's own terms: [`Lookups`]. Asked for by
//! [`crate::ParseOptions::lookups`]; read by the verify task's guard L1
//! (`spec/verify.md` §5.3, work package WP-052) and by the map task for
//! the shard's negative inputs (`spec/model.md` §8.6, work package WP-117).

use std::collections::HashSet;
use std::ops::Range;

use kiln_driver::front_end::FrontEnd;
use kiln_pp::{
    Characteristic, DirGroup, Found, HeaderFs, LookupForm, LookupLog, RealFs, SearchDirSpec,
    SearchList, join_path,
};

use crate::kiln::source::{FileId, FileOrigin, SourceManager};

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
    /// driver left out of the search list, because it was no directory or
    /// was a duplicate of a listed one (another spelling of it, a link to
    /// it), is a place for each name a `#include` form looked for in the
    /// list where that directory, were it a directory of its own, would be
    /// searched: at its place in the list (the command's list built with
    /// it), from where the lookup started to the directory it found the
    /// name in (through the end when it found none), unless the name is a
    /// file in it now (a duplicate's twin holds it). Two exceptions keep
    /// more: an `-isystem` or `-idirafter` one is a place of every lookup
    /// that ended in an `-I` directory (created as that directory, it
    /// would take its place); and when its place cannot be told (a list
    /// that is not the command's own with one toolchain block, or a
    /// directory the C++ build of the list leaves out), it is a place of
    /// every lookup in the list, but for a `<...>` name when only
    /// `-iquote` names it.
    pub missed: Vec<(String, String)>,
    /// Each place `(directory, name)` where a recorded lookup **found** its
    /// file, unique, in the order first seen (work package WP-135): the
    /// file Kiln opened is `directory/name` as the log spells it (Kiln
    /// `HeaderSearch::lookup` opens `join_path(dir, name)`), the includer's
    /// directory for a hit beside the includer (`Found::Includer`; `.` when
    /// the log names no includer, Kiln's own rule), the search list's entry
    /// for a hit in the list (`Found::Dir`); for `#embed` and `__has_embed`,
    /// `(".", path)` with the path read (`Found::Embed`); for an absolute
    /// name, `("/", name)` (Kiln's log does not record one yet).
    pub found: Vec<(String, String)>,
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
    /// its group.
    pub(crate) dirs: Vec<(DirGroup, String)>,
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
                (*g, path)
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
        // The command's search directories the driver left out of the
        // list: no directory, or a duplicate of a listed one (another
        // spelling of it, or a link to it, which may be retargeted since).
        let listed: HashSet<&str> = list.iter().map(String::as_str).collect();
        let mut absent_paths: Vec<&str> = Vec::new();
        for (_, d) in &driver.dirs {
            if !listed.contains(d.as_str()) && !absent_paths.contains(&d.as_str()) {
                absent_paths.push(d);
            }
        }
        let absent = place_absent(&list, log.angled_start as usize, &driver.dirs, &absent_paths);
        let mut tried = Places::default();
        let mut missed = Places::default();
        let mut found = Places::default();
        for l in &log.lookups {
            // An absolute name is tried where it is, whatever the form
            // (Kiln's log does not record one yet: `spec/verify.md` §5.1,
            // "What remains outside").
            if l.name.starts_with('/') {
                tried.push("/", &l.name);
                if matches!(l.found, Found::Missing) {
                    missed.push("/", &l.name);
                } else {
                    found.push("/", &l.name);
                }
                continue;
            }
            if l.form == LookupForm::Embed {
                embed(l, log, &driver.embed_dirs, &mut tried, &mut missed);
                if let Found::Embed { path } = &l.found {
                    found.push(".", path);
                }
                continue;
            }
            if let Some(i) = l.includer {
                tried.push(dir(log, i), &l.name);
                if l.found == Found::Includer {
                    found.push(dir(log, i), &l.name);
                    continue;
                }
                missed.push(dir(log, i), &l.name);
            } else if l.found == Found::Includer {
                found.push(".", &l.name);
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
            if let Found::Dir { dir: d, .. } = l.found {
                found.push(dir(log, d), &l.name);
            }
            let end = hit.map_or(log.list.len(), |h| h + 1);
            for &d in &log.list[start..end] {
                tried.push(dir(log, d), &l.name);
            }
            for &d in &log.list[start..hit.unwrap_or(end)] {
                missed.push(dir(log, d), &l.name);
            }
            for a in &absent {
                if a.searched(l.angled, start, hit) && !a.holds(&l.name) {
                    missed.push(&a.path, &l.name);
                }
            }
        }
        let mut out = Lookups {
            list,
            tried: tried.out,
            missed: missed.out,
            found: found.out,
            ..Lookups::default()
        };
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

/// A search directory of the command that Kiln's driver left out of the
/// list, because it was no directory or was a duplicate of a listed one,
/// and where in the list it would be searched were it a directory of its
/// own (`spec/model.md` §8.4).
struct Absent {
    /// As the command spells it.
    path: String,
    /// It is a directory now (a duplicate): a name in it that is a file
    /// now is no place where nothing was found.
    exists: bool,
    /// Each place in the list it would take, `None` when that cannot be
    /// told: then it is a negative of every lookup in the list.
    slots: Option<Vec<Slot>>,
    /// Every word naming it is `-iquote`.
    quoted_only: bool,
    /// A word naming it is of a system group (`-isystem`, `-idirafter`):
    /// created as an `-I` directory of the list, it would take that
    /// directory's place (clang `RemoveDuplicates`).
    system: bool,
    /// The positions of the `-I` directories in the list.
    user: Range<usize>,
}

/// One place an absent directory would take: between the list's entries
/// `at - 1` and `at`, for an `at` in `lo..=hi` (two when the list cannot
/// tell its system directories from its `-idirafter` ones).
struct Slot {
    lo: usize,
    hi: usize,
    /// In the `-iquote` part, which `<...>` lookups do not search.
    quoted: bool,
}

impl Absent {
    /// Whether `name` is a regular file in this directory now (only a
    /// duplicate can hold one; the file is then its twin's, which the
    /// lookup searched).
    fn holds(&self, name: &str) -> bool {
        self.exists && std::fs::metadata(join_path(&self.path, name)).is_ok_and(|m| m.is_file())
    }

    /// Whether a lookup in the list, from `start`, that ended at the list's
    /// entry `hit` (`None`: through the end) would look in this directory.
    fn searched(&self, angled: bool, start: usize, hit: Option<usize>) -> bool {
        let Some(slots) = &self.slots else {
            return !(angled && self.quoted_only);
        };
        if self.system && hit.is_some_and(|h| self.user.contains(&h)) {
            return true;
        }
        slots
            .iter()
            .any(|s| !(angled && s.quoted) && s.hi >= start && hit.is_none_or(|h| s.lo <= h))
    }
}

/// The places of each of `absent` in `list` (the log's search list, whose
/// `<...>` part starts at `angled_start`), from the command's directories
/// `dirs` built into a list as Kiln's driver builds it (Kiln
/// `SearchList::build`, for C++). The command's directories come first in
/// each group and the toolchain's are all system ones, so `list` must be
/// the command's own list with one block of toolchain directories at the
/// end of its system part, none of them one of the command's (an
/// `-iquote` directory that is also a toolchain one is listed twice, in
/// the two parts Kiln deduplicates apart: then no directory is placed).
/// When it is not (a toolchain directory that displaced one of the
/// command's, or a C unit's `-cxx-isystem` directories, which the C++
/// build lists and Kiln's driver leaves out for C), no directory is
/// placed. Kiln's driver accepts no `-c-isystem` word (the unit is
/// `rejected`), so no `CSystem` directory comes here.
///
/// Building the list with `p` a directory of its own changes no other
/// entry: `p`'s identity is then unique, so it duplicates nothing, and
/// whatever Kiln dropped because of `p` was `p` itself (it is not in the
/// log's list), never its twin (clang `RemoveDuplicates` drops either
/// the later one or the user one, and the one kept is listed). So the
/// command's list with `p` is the command's list with `p` inserted.
fn place_absent(
    list: &[String],
    angled_start: usize,
    dirs: &[(DirGroup, String)],
    absent: &[&str],
) -> Vec<Absent> {
    let frame = Frame::of(list, angled_start, dirs);
    absent
        .iter()
        .map(|&p| {
            let groups: Vec<DirGroup> =
                dirs.iter().filter(|(_, d)| d == p).map(|(g, _)| *g).collect();
            Absent {
                path: p.to_owned(),
                exists: std::fs::metadata(p).is_ok_and(|m| m.is_dir()),
                slots: frame.as_ref().and_then(|f| f.slots(p, &groups)),
                quoted_only: groups.iter().all(|&g| g == DirGroup::Quoted),
                system: groups.iter().any(|&g| Characteristic::of_group(g).is_system()),
                user: frame.as_ref().map_or(0..0, |f| f.user.clone()),
            }
        })
        .collect()
}

/// The command's own search list, and how the log's list extends it.
struct Frame {
    specs: Vec<SearchDirSpec>,
    /// How many of the command's own list are before its `-idirafter`
    /// directories.
    before_after: usize,
    /// How many toolchain directories the log's list has there.
    toolchain: usize,
    /// The positions of the `-I` directories in the log's list.
    user: Range<usize>,
}

fn paths(l: &SearchList) -> Vec<String> {
    l.dirs().iter().map(|d| d.path.clone()).collect()
}

impl Frame {
    fn of(list: &[String], angled_start: usize, dirs: &[(DirGroup, String)]) -> Option<Frame> {
        let specs: Vec<SearchDirSpec> =
            dirs.iter().map(|(g, d)| SearchDirSpec::new(d.clone(), *g)).collect();
        let full = SearchList::build(&specs, true, &mut RealFs);
        let own = paths(&full);
        let no_after: Vec<SearchDirSpec> =
            specs.iter().filter(|s| s.group != DirGroup::After).cloned().collect();
        let head = paths(&SearchList::build(&no_after, true, &mut RealFs));
        let k = head.len();
        if full.angled_start() != angled_start || k > own.len() || head[..] != own[..k] {
            return None;
        }
        let t = list.len().checked_sub(own.len())?;
        let mine: HashSet<&str> = own.iter().map(String::as_str).collect();
        if list[..k] != own[..k]
            || list[k + t..] != own[k..]
            || list[k..k + t].iter().any(|d| mine.contains(d.as_str()))
        {
            return None;
        }
        let user_end = full.dirs().iter().take_while(|d| !d.characteristic.is_system()).count();
        Some(Frame { specs, before_after: k, toolchain: t, user: angled_start..user_end })
    }

    /// The places `p` (named by words of `groups`) would take in the log's
    /// list were it a directory, from the command's list built with it.
    fn slots(&self, p: &str, groups: &[DirGroup]) -> Option<Vec<Slot>> {
        let with = SearchList::build(&self.specs, true, &mut Pretend { path: p });
        let names = paths(&with);
        let has_after = groups.contains(&DirGroup::After);
        let only_after = groups.iter().all(|&g| g == DirGroup::After);
        let (k, t) = (self.before_after, self.toolchain);
        let mut slots = Vec::new();
        for (i, d) in names.iter().enumerate() {
            if d != p {
                continue;
            }
            let at = i - slots.len();
            let (lo, hi) = match at.cmp(&k) {
                std::cmp::Ordering::Less => (at, at),
                std::cmp::Ordering::Greater => (at + t, at + t),
                std::cmp::Ordering::Equal if !has_after => (at, at),
                std::cmp::Ordering::Equal if only_after => (at + t, at + t),
                std::cmp::Ordering::Equal => (at, at + t),
            };
            slots.push(Slot { lo, hi, quoted: i < with.angled_start() });
        }
        (!slots.is_empty()).then_some(slots)
    }
}

/// The disk, with `path` a directory of its own.
struct Pretend<'a> {
    path: &'a str,
}

impl HeaderFs for Pretend<'_> {
    fn dir_identity(&mut self, path: &str) -> Option<String> {
        if path == self.path {
            return Some(format!("\0absent {path}"));
        }
        RealFs.dir_identity(path)
    }

    fn open(&mut self, sm: &SourceManager, path: &str) -> Option<FileId> {
        RealFs.open(sm, path)
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        RealFs.read(path)
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
    let mut order: Vec<(&str, String)> = Vec::new();
    if let Some(i) = l.includer {
        order.push((dir(log, i), join_path(dir(log, i), &l.name)));
    }
    if !l.angled {
        order.push((".", l.name.clone()));
    }
    order.extend(embed_dirs.iter().map(|d| (d.as_str(), join_path(d, &l.name))));
    for (d, _) in &order {
        tried.push(d, &l.name);
    }
    let read = match &l.found {
        Found::Embed { path } => order.iter().position(|(_, p)| p == path),
        _ => None,
    };
    // A path read that is none of the tries cannot be placed: every try is
    // then missed, which refuses more, never less.
    for (d, _) in &order[..read.unwrap_or(order.len())] {
        missed.push(d, &l.name);
    }
}

/// The directory name `i` of `log` (`""` out of range: Kiln's own log is
/// consistent, and an empty name is the working directory, which the guard
/// then checks).
fn dir(log: &LookupLog, i: u32) -> &str {
    log.dirs.get(i as usize).map_or("", String::as_str)
}
