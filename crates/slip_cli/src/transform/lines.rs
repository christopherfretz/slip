//! The lines of `spec/edits.md` §10.5 that describe an edit set: the
//! summary (`edits` to `declared`) and the details (`refusal`, `report`,
//! `failed run`, `exempt`). Every write ignores its error (a closed pipe
//! must not become a panic, `crate::commands`).
//!
//! Paths are written as their bytes; a message, a display, a reason and a
//! macro name are written with every invalid UTF-8 sequence replaced by
//! U+FFFD and every control character below U+0020 by a space (§10.5).

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use slip_model::declared::Declared;
use slip_model::edits::{Reason, Severity};
use slip_model::editset::{Cause, EditSet, SetState};
use slip_model::hash::content_digest;
use slip_model::{Digest, FileRef, Plan};

/// The most lines of each kind of detail (§10.5).
pub const DETAIL_MAX: usize = 200;

/// `bytes` as text that stays on its line (§10.5).
pub fn printable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().map(|c| if c < ' ' { ' ' } else { c }).collect()
}

/// A real path, relative to `relative_to` (a canonical directory) when it
/// is inside it (§10.5: "paths relative to `--relative-to` when given");
/// a path outside it is written whole.
pub fn shown_path<'p>(path: &'p [u8], relative_to: Option<&[u8]>) -> &'p [u8] {
    match relative_to {
        None => path,
        Some(b"/") => path.strip_prefix(b"/").unwrap_or(path),
        Some(dir) => match path.strip_prefix(dir) {
            Some([b'/', rest @ ..]) if !rest.is_empty() => rest,
            _ => path,
        },
    }
}

/// The counts of the `edit` line, from the set's edits (§10.5, the
/// meanings of `spec/edits.md` §6.3): edits, files with one, edits made by
/// more than one observer, the largest `observers`, and edits that are not
/// outermost (inside another edit of their file).
fn edit_counts(set: &EditSet) -> (usize, usize, usize, u64, usize) {
    let mut files = 0;
    let mut shared = 0;
    let mut most = 0;
    let mut nested = 0;
    let mut i = 0;
    while i < set.edits.len() {
        let file = set.edits[i].file;
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        while i < set.edits.len() && set.edits[i].file == file {
            let e = &set.edits[i];
            ranges.push((e.lo, e.hi));
            if e.observers > 1 {
                shared += 1;
            }
            most = most.max(e.observers);
            i += 1;
        }
        files += 1;
        // Outer before inner: by `lo`, then the larger range first. No two
        // edits have one range (§6), so an edit is inside an earlier one
        // exactly when an earlier one reaches as far.
        ranges.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut reach: Option<u64> = None;
        for (_, hi) in ranges {
            if reach.is_some_and(|r| r >= hi) {
                nested += 1;
            }
            reach = Some(reach.map_or(hi, |r| r.max(hi)));
        }
    }
    (set.edits.len(), files, shared, most, nested)
}

fn counter(set: &EditSet, name: &str) -> u64 {
    set.counters.iter().find(|c| c.name == name).map_or(0, |c| c.value)
}

/// The summary lines of §10.5 for an edit set, from `edits <id>` to
/// `declared <id>: …`. `indivisible`: the request's (the `groups` line is
/// omitted for an indivisible request).
pub fn edit_set_summary(
    set_id: Digest,
    set: &EditSet,
    declared: Option<(Digest, &Declared)>,
    indivisible: bool,
    out: &mut dyn Write,
) {
    let _ = writeln!(out, "edits {set_id}");
    let _ = writeln!(out, "state {}", set.state.name());
    let (edits, files, shared, most, nested) = edit_counts(set);
    let _ = writeln!(
        out,
        "edit {edits} in {files} file(s); shared {shared}; most observers {most}; nested {nested}"
    );
    if !indivisible {
        let _ = writeln!(
            out,
            "groups {}, rejected {}",
            counter(set, "reduce.groups"),
            counter(set, "reduce.groups.rejected")
        );
    }
    let _ = writeln!(out, "kept {}; unresolved {}", set.kept.len(), set.unresolved.len());
    let by = |s: Severity| set.reports.iter().filter(|r| r.report.severity == s).count();
    let (note, review, error) = (by(Severity::Note), by(Severity::Review), by(Severity::Error));
    if note + review + error > 0 {
        let _ = writeln!(out, "reports: note {note}, review {review}, error {error}");
    }
    for cause in Cause::ALL {
        let n = set.refusals.iter().filter(|r| r.cause == cause).count();
        if n > 0 {
            let _ = writeln!(out, "refused {} {n}", cause.name());
        }
    }
    if let Some((id, d)) = declared {
        let _ = writeln!(
            out,
            "declared {id}: rules {}, exempt {}",
            d.rules().rules().len(),
            d.changed().len()
        );
    }
    debug_assert!(set.state == SetState::Accepted || set.edits.is_empty());
}

/// Lines and columns of the set's files, read from the pre-image on disk
/// once per file (§10.5).
struct Places<'a> {
    files: &'a dyn slip_apply::Files,
    refs: &'a [FileRef],
    relative_to: Option<&'a [u8]>,
    /// Per file index: the offsets of its `\n` bytes, or `None` when the
    /// file on disk is not the `FileRef`'s bytes.
    newlines: HashMap<u64, Option<Vec<u64>>>,
}

impl Places<'_> {
    /// `<path>:<line>:<column>` (1-based, the column in bytes) of offset
    /// `lo` of file `file`, or `<path>:@<lo>` when the file on disk is not
    /// the one the set names.
    fn place(&mut self, file: u64, lo: u64) -> Vec<u8> {
        let Some(f) = usize::try_from(file).ok().and_then(|i| self.refs.get(i)) else {
            return format!("<file {file}>:@{lo}").into_bytes();
        };
        let (files, f2) = (self.files, f);
        let newlines = self.newlines.entry(file).or_insert_with(|| {
            let bytes = files.read(&f2.path).ok()?;
            let same = bytes.len() as u64 == f2.size && content_digest(&bytes) == f2.content;
            same.then(|| {
                bytes
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b == b'\n')
                    .map(|(i, _)| i as u64)
                    .collect()
            })
        });
        let mut out = shown_path(&f.path, self.relative_to).to_vec();
        match newlines {
            Some(nl) if lo <= f.size => {
                let before = nl.partition_point(|&n| n < lo);
                let start = if before == 0 { 0 } else { nl[before - 1] + 1 };
                out.extend_from_slice(format!(":{}:{}", before + 1, lo - start + 1).as_bytes());
            }
            _ => out.extend_from_slice(format!(":@{lo}").as_bytes()),
        }
        out
    }
}

/// ` (<n> observer(s), first <main file>)`: `first` is the first of the
/// record's `first` observers, by its main file in the plan (its id when
/// the plan does not have it); omitted when the list is empty.
fn observers_part(
    n: u64,
    first: &[Digest],
    mains: &HashMap<Digest, &[u8]>,
    relative_to: Option<&[u8]>,
) -> Vec<u8> {
    let mut out = format!(" ({n} observer(s)").into_bytes();
    if let Some(o) = first.first() {
        out.extend_from_slice(b", first ");
        match mains.get(o) {
            Some(path) => out.extend_from_slice(shown_path(path, relative_to)),
            None => out.extend_from_slice(o.to_string().as_bytes()),
        }
    }
    out.push(b')');
    out
}

/// Writes the lines of one kind, at most [`DETAIL_MAX`], then the `more`
/// line if any were cut.
fn write_capped(
    kind: &str,
    lines: impl ExactSizeIterator<Item = Vec<u8>>,
    record: &Path,
    out: &mut dyn Write,
) {
    let total = lines.len();
    for line in lines.take(DETAIL_MAX) {
        let _ = out.write_all(&line);
        let _ = out.write_all(b"\n");
    }
    if total > DETAIL_MAX {
        let _ = write!(out, "{kind}: {} more (slip dump ", total - DETAIL_MAX);
        let _ = out.write_all(&crate::args::os_bytes(record.as_os_str()));
        let _ = writeln!(out, ")");
    }
}

/// The `refusal`, `report`, `failed run` and `exempt` lines of §10.5, each
/// list in the order of its record and cut at [`DETAIL_MAX`], followed by
/// `<kind>: <n> more (slip dump <record>)`; `record` is the file of the
/// edit set. Lines and columns come from `files`, the pre-image.
pub fn edit_set_details(
    set: &EditSet,
    plan: &Plan,
    relative_to: Option<&[u8]>,
    files: &dyn slip_apply::Files,
    record: &Path,
    out: &mut dyn Write,
) {
    let mains: HashMap<Digest, &[u8]> =
        plan.observers.iter().map(|o| (o.id, o.main.path.as_slice())).collect();
    let displays: HashMap<Digest, &str> =
        set.subjects.iter().map(|s| (s.id, s.display.as_str())).collect();
    let mut places = Places { files, refs: &set.files, relative_to, newlines: HashMap::new() };

    let refusals = set.refusals.iter().map(|r| {
        let mut line = format!("refusal {}", r.cause.name()).into_bytes();
        if r.reason != Reason::None {
            line.extend_from_slice(format!(" {}", r.reason.name()).as_bytes());
        }
        line.push(b':');
        let display = r
            .subject
            .map(|s| displays.get(&s).map_or_else(|| s.to_string(), |d| printable(d.as_bytes())));
        if let Some(file) = r.file {
            line.push(b' ');
            line.extend_from_slice(&places.place(file, r.lo));
            if let Some(d) = &display {
                line.extend_from_slice(b": ");
                line.extend_from_slice(d.as_bytes());
            }
            if let Some(m) = &r.macro_site {
                line.extend_from_slice(format!(" in macro {}", printable(&m.name)).as_bytes());
            }
        } else if let Some(d) = &display {
            line.push(b' ');
            line.extend_from_slice(d.as_bytes());
        } else {
            // Neither a site nor a subject (`no_edit`): the cause alone.
            line.pop();
        }
        line.extend_from_slice(&observers_part(r.observers, &r.first, &mains, relative_to));
        line
    });
    write_capped("refusal", refusals, record, out);

    let reports = set.reports.iter().map(|r| {
        let mut line = format!("report {}: ", r.report.severity.name()).into_bytes();
        if let Some(p) = &r.report.place {
            line.extend_from_slice(&places.place(p.file, p.lo));
            if r.report.attached {
                line.extend_from_slice(b" (nearest place)");
            }
            line.extend_from_slice(b": ");
        }
        line.extend_from_slice(printable(&r.report.message).as_bytes());
        line.extend_from_slice(&observers_part(r.observers, &r.first, &mains, relative_to));
        line
    });
    write_capped("report", reports, record, out);

    let failures = set.failures.iter().map(|f| {
        let mut line = format!(
            "failed run {}: {}",
            f.failure.kind.name(),
            printable(f.failure.message.as_bytes())
        )
        .into_bytes();
        line.extend_from_slice(&observers_part(f.observers, &f.first, &mains, relative_to));
        line
    });
    write_capped("failed run", failures, record, out);

    let exempt = set.changed.iter().map(|x| {
        let key = x.key.to_string();
        let mut line = format!("exempt {} {}: ", &key[..16], printable(&x.reason)).into_bytes();
        line.extend_from_slice(&places.place(x.place.file, x.place.lo));
        if !x.in_force {
            line.extend_from_slice(b" (not in force)");
        }
        line
    });
    write_capped("exempt", exempt, record, out);
}
