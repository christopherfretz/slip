//! What a unit's header search looked at, from Kiln's lookup log
//! (Kiln `kiln_pp/src/lookups.rs`, Kiln `spec/pp.md` "Lookup log"), in
//! Slip's own terms: [`Lookups`]. Asked for by
//! [`crate::ParseOptions::lookups`]; read by the verify task's guard L1
//! (`spec/verify.md` §5.3, work package WP-052).

use std::collections::HashSet;

use kiln_driver::front_end::FrontEnd;
use kiln_pp::{Found, LookupForm, LookupLog};

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
    /// The name of every buffer read from disk and of every consulted path
    /// (each of the shard's `inputs`, as spelled when it was read).
    pub read: Vec<String>,
}

impl Lookups {
    /// The lookups of `fe`, whose log Kiln recorded (`None` without one).
    /// `embed_dirs` is called only when the log has an `#embed` form.
    pub(crate) fn of(fe: &FrontEnd<'_, '_>, embed_dirs: impl FnOnce() -> Vec<String>) -> Option<Self> {
        let log = fe.lookups?;
        let mut out = Lookups {
            list: log.list.iter().map(|&d| dir(log, d).to_owned()).collect(),
            ..Lookups::default()
        };
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut push = |out: &mut Lookups, d: &str, name: &str| {
            let key = (d.to_owned(), name.to_owned());
            if seen.insert(key.clone()) {
                out.tried.push(key);
            }
        };
        let mut embed_dirs = Some(embed_dirs);
        let mut embeds: Option<Vec<String>> = None;
        for l in &log.lookups {
            if let Some(i) = l.includer {
                push(&mut out, dir(log, i), &l.name);
            }
            if l.form == LookupForm::Embed {
                if !l.angled {
                    push(&mut out, ".", &l.name);
                }
                let dirs = embeds.get_or_insert_with(|| embed_dirs.take().map_or_else(Vec::new, |f| f()));
                for d in dirs.iter() {
                    push(&mut out, d, &l.name);
                }
                continue;
            }
            if l.found == Found::Includer {
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
            let end = match l.found {
                Found::Dir { dir: hit, .. } => log.list[start..]
                    .iter()
                    .position(|&d| d == hit)
                    .map_or(log.list.len(), |p| start + p + 1),
                _ => log.list.len(),
            };
            for &d in &log.list[start..end] {
                push(&mut out, dir(log, d), &l.name);
            }
        }
        for file in fe.sm.file_ids() {
            let read = match fe.sm.file_origin(file) {
                FileOrigin::Virtual => false,
                FileOrigin::Disk => true,
                FileOrigin::Lazy => fe.sm.original_bytes(file).is_some(),
            };
            if read {
                out.read.push(fe.sm.file_name(file).to_owned());
            }
        }
        out.read.extend(fe.consulted.iter().map(|c| c.path.clone()));
        Some(out)
    }
}

/// The directory name `i` of `log` (`""` out of range: Kiln's own log is
/// consistent, and an empty name is the working directory, which the guard
/// then checks).
fn dir(log: &LookupLog, i: u32) -> &str {
    log.dirs.get(i as usize).map_or("", String::as_str)
}
