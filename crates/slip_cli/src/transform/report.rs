//! The lines of `spec/edits.md` §10.5 that describe a result: the summary
//! (`verify:` to `state`) and the `finding`, `stale`, `spelling` and
//! `residue` details. Every write ignores its error (`crate::commands`).
//! The records hold everything; the lines are bounded (64 `finding`
//! groups, [`DETAIL_MAX`] lines of each other kind, each cut announced).

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use slip_exec::store::Store;
use slip_model::declared::Declared;
use slip_model::editset::EditSet;
use slip_model::json::Value;
use slip_model::result::{Category, TransformResult};
use slip_model::{Digest, Overlay, Plan};
use slip_verify::run::Report;

use super::lines::{
    edit_set_exempt_details, edit_set_head_details, printable, shown_path, write_capped,
};
use super::store::DiskFiles;

/// The most `finding` groups printed (§10.5).
pub const FINDING_GROUPS_MAX: usize = 64;

/// The summary lines from `verify:` to `state` (§10.5). `scanned`: the scan
/// step ran (a rename's run that was not cut short); the `verify:`,
/// `agreement:` and `spellings:` lines are those of a verify phase that
/// finished (a verdict).
pub fn result_summary(
    result_id: Digest,
    result: &TransformResult,
    scanned: bool,
    out: &mut dyn Write,
) {
    if result.verdict.is_some() {
        let t = &result.tally;
        let _ = writeln!(
            out,
            "verify: verified {}, unchanged {}, mismatch {}, not_parsed {}, unverifiable {}, \
             never_parsed {}, failed {}",
            t.verified,
            t.unchanged,
            t.mismatch,
            t.not_parsed,
            t.unverifiable,
            t.never_parsed,
            t.failed
        );
        let _ = writeln!(
            out,
            "agreement: stale {}, unused rules {}",
            result.agreement.len(),
            result.unused.len()
        );
        let _ = writeln!(
            out,
            "spellings: passed {}, failed {}",
            result.spellings.passed,
            result.spellings.failed.len()
        );
    }
    if scanned {
        let n = |c: Category| result.residue.iter().filter(|r| r.category == c).count();
        let _ = writeln!(
            out,
            "residue: dependent {}, not_compiled {}, not_planned {}; mentions {}",
            n(Category::Dependent),
            n(Category::NotCompiled),
            n(Category::NotPlanned),
            n(Category::Mention)
        );
    }
    let _ = writeln!(out, "result {result_id}");
    let _ = writeln!(out, "state {}", result.state.name());
}

/// What [`details`] prints from.
#[derive(Debug, Clone, Copy)]
pub struct Details<'a> {
    pub store: &'a Store,
    pub plan: &'a Plan,
    pub set: &'a EditSet,
    pub set_record: &'a Path,
    pub declared: Option<&'a Declared>,
    pub result: &'a TransformResult,
    pub result_record: &'a Path,
    pub overlay: Option<&'a Overlay>,
    /// The verify phase's report, if it finished.
    pub report: Option<&'a Report>,
    /// `--report <file>`, named when the `finding` lines are cut.
    pub report_file: Option<&'a Path>,
    /// A canonical directory (`--relative-to`).
    pub relative_to: Option<&'a [u8]>,
}

/// The detail lines of §10.5 in its order: `refusal`, `report`, `failed
/// run` (the edit set's), `finding`, `stale`, `spelling`, `exempt`, the
/// `instances:` heading and `instance` lines (the edit set's), `residue`.
pub fn details(d: &Details<'_>, out: &mut dyn Write) {
    let rel = d.relative_to;
    edit_set_head_details(d.set, d.plan, rel, &DiskFiles, d.set_record, out);
    if let Some(r) = d.report {
        findings(r, rel, d.report_file, out);
    }
    let mains: HashMap<Digest, &[u8]> =
        d.plan.observers.iter().map(|o| (o.id, o.main.path.as_slice())).collect();
    let rules = d.declared.map_or(&[][..], |x| x.rules().rules());
    let stale = d.result.agreement.iter().map(|s| {
        let rule = usize::try_from(s.rule).ok().and_then(|i| rules.get(i));
        let display = rule
            .and_then(|r| d.set.subjects.iter().find(|x| x.rule.as_ref() == Some(r)))
            .map_or_else(|| format!("rule {}", s.rule), |x| printable(x.display.as_bytes()));
        let mut line =
            format!("stale {display}: still declared under its old name in ").into_bytes();
        match mains.get(&s.observer) {
            Some(m) => line.extend_from_slice(shown_path(m, rel)),
            None => line.extend_from_slice(s.observer.to_string().as_bytes()),
        }
        line
    });
    write_capped("stale", stale, d.result_record, out);
    if let Some(o) = d.overlay {
        let mut posts = PostLines { store: d.store, overlay: o, newlines: HashMap::new() };
        let spellings = d.result.spellings.failed.iter().map(|f| {
            let ob = usize::try_from(f.obligation).ok().and_then(|i| o.spellings.get(i));
            let mut line = format!("spelling {}: ", f.why.name()).into_bytes();
            if let Some(ob) = ob {
                line.extend_from_slice(&posts.place(ob.file, ob.lo, rel));
                let key = ob.key.to_string();
                line.extend_from_slice(format!(": meant {}", &key[..16]).as_bytes());
            }
            if !f.name.is_empty() {
                line.extend_from_slice(
                    format!(", found {}", printable(f.name.as_bytes())).as_bytes(),
                );
            } else if let Some(k) = f.found {
                line.extend_from_slice(format!(", found {}", &k.to_string()[..16]).as_bytes());
            }
            line.extend_from_slice(format!(" ({} observer(s))", f.observers).as_bytes());
            line
        });
        write_capped("spelling", spellings, d.result_record, out);
    }
    edit_set_exempt_details(d.set, d.plan, rel, &DiskFiles, d.set_record, out);
    let residue = d.result.residue.iter().map(|r| {
        let mut line = format!("residue {}: ", r.category.name()).into_bytes();
        line.extend_from_slice(shown_path(&r.file, rel));
        line.extend_from_slice(format!(":{}:{}", r.line, r.column).as_bytes());
        line
    });
    write_capped("residue", residue, d.result_record, out);
}

/// Line and column of an offset of an overlay file's post-image, from its
/// blob, read once per file.
struct PostLines<'a> {
    store: &'a Store,
    overlay: &'a Overlay,
    newlines: HashMap<u64, Option<Vec<u64>>>,
}

impl PostLines<'_> {
    fn place(&mut self, file: u64, lo: u64, rel: Option<&[u8]>) -> Vec<u8> {
        let Some(f) = usize::try_from(file).ok().and_then(|i| self.overlay.files.get(i)) else {
            return format!("<file {file}>:@{lo}").into_bytes();
        };
        let store = self.store;
        let nl = self.newlines.entry(file).or_insert_with(|| {
            let bytes = store.read_blob(&f.post.content).ok()??;
            Some(newlines(&bytes))
        });
        let mut out = shown_path(&f.path, rel).to_vec();
        out.extend_from_slice(&line_column(nl.as_deref(), lo, f.post.size));
        out
    }
}

/// The offsets of the `\n` bytes of `bytes`.
pub(crate) fn newlines(bytes: &[u8]) -> Vec<u64> {
    bytes.iter().enumerate().filter(|(_, b)| **b == b'\n').map(|(i, _)| i as u64).collect()
}

/// `:<line>:<column>` (1-based, the column in bytes) of offset `lo` of a
/// file of `size` bytes whose `\n` offsets are `nl`; `:@<lo>` when the
/// file could not be read.
fn line_column(nl: Option<&[u64]>, lo: u64, size: u64) -> Vec<u8> {
    match nl {
        Some(nl) if lo <= size => {
            let before = nl.partition_point(|&n| n < lo);
            let start = if before == 0 { 0 } else { nl[before - 1] + 1 };
            format!(":{}:{}", before + 1, lo - start + 1).into_bytes()
        }
        _ => format!(":@{lo}").into_bytes(),
    }
}

/// The bytes of a path in the report's JSON: a string, or `{"hex": …}`.
fn json_bytes(v: Option<&Value>) -> Vec<u8> {
    match v {
        Some(Value::Str(s)) => s.as_bytes().to_vec(),
        Some(o @ Value::Object(_)) => {
            let hex = o.get("hex").and_then(Value::as_str).unwrap_or("");
            (0..hex.len() / 2)
                .filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The `finding` lines: the report's groups (`spec/verify.md` §7.1), at
/// most [`FINDING_GROUPS_MAX`]; the place is the pre-image's, else the
/// post-image's, when the finding has one.
fn findings(report: &Report, rel: Option<&[u8]>, file: Option<&Path>, out: &mut dyn Write) {
    let json = report.to_json();
    let groups =
        json.get("findings").and_then(|f| f.get("groups")).and_then(Value::as_array).unwrap_or(&[]);
    for g in groups.iter().take(FINDING_GROUPS_MAX) {
        let text = |k: &str| g.get(k).and_then(Value::as_str).unwrap_or("");
        let n = g.get("observers").and_then(Value::as_u64).unwrap_or(0);
        let mut line = format!(
            "finding {} {}: {n} observer(s), first ",
            text("kind"),
            printable(text("name").as_bytes())
        )
        .into_bytes();
        line.extend_from_slice(shown_path(&json_bytes(g.get("first")), rel));
        let site = ["pre", "post"].into_iter().find_map(|side| {
            let s = g.get("where")?.get(side)?;
            let l = s.get("line")?.as_u64().filter(|&l| l > 0)?;
            Some((json_bytes(s.get("file")), l, s.get("column")?.as_u64()?))
        });
        if let Some((path, l, c)) = site {
            line.extend_from_slice(b": ");
            line.extend_from_slice(shown_path(&path, rel));
            line.extend_from_slice(format!(":{l}:{c}").as_bytes());
        }
        let _ = out.write_all(&line);
        let _ = out.write_all(b"\n");
    }
    if groups.len() > FINDING_GROUPS_MAX {
        let _ = write!(out, "finding: {} more (", groups.len() - FINDING_GROUPS_MAX);
        match file {
            Some(f) => {
                let _ = write!(out, "the report {}", f.display());
            }
            None => {
                let _ = write!(out, "--report <file> writes them all");
            }
        }
        let _ = writeln!(out, ")");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column() {
        let nl = newlines(b"ab\ncd\n\nx");
        assert_eq!(line_column(Some(&nl), 0, 8), b":1:1");
        assert_eq!(line_column(Some(&nl), 4, 8), b":2:2");
        assert_eq!(line_column(Some(&nl), 7, 8), b":4:1");
        assert_eq!(line_column(None, 7, 8), b":@7");
    }

    #[test]
    fn bytes_of_json() {
        assert_eq!(json_bytes(Some(&Value::Str("/a".into()))), b"/a");
        let hex = Value::Object(vec![("hex".into(), Value::Str("2fff".into()))]);
        assert_eq!(json_bytes(Some(&hex)), b"/\xff");
    }
}
