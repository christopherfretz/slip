//! `slip units` (`spec/exec.md` §8.9): the unit records of the selected
//! observers, for a person writing a declared file or reading a report.
//! Work package WP-053. The selection, its bound and the in-process parses
//! are `declare`'s (`crate::declare::InProcess`).

use std::io::Write;
use std::process::ExitCode;

use slip_digest::records::Record;
use slip_model::digests::GroupClass;
use slip_model::keys::RuleTable;

use crate::args::UnitsArgs;
use crate::commands::{self, contains};
use crate::declare::{InProcess, Merged};
use crate::image::Side;
use crate::status;
use crate::verify_cmd::escape;

/// `slip units` (§8.9).
pub fn main(args: &UnitsArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let result = units(args, out, err);
    commands::finish(result, out, err)
}

fn units(a: &UnitsArgs, out: &mut dyn Write, err: &mut dyn Write) -> commands::Result<u8> {
    let run = InProcess::open("units", &a.plan, &a.image, &a.store, &a.only, a.limit)?;
    let side = if a.post_image { Side::Post } else { Side::Pre };
    let none = RuleTable::empty();
    let mut merged = Merged::default();
    for o in &run.observers {
        if let Some(records) = run.records("units", o, side, &none, None, err)? {
            merged.add(records);
        }
    }
    let mut records: Vec<Record> = merged
        .records
        .into_iter()
        .filter(|r| {
            a.matches.is_empty() || a.matches.iter().any(|m| contains(r.name.as_bytes(), m))
        })
        .collect();
    records.sort_by(|x, y| (&x.name, &x.id).cmp(&(&y.name, &y.id)));
    // One line per record; records of one `KeyId` from several observers
    // are one line (two records that differ only in what the line does not
    // show print the same line once).
    let mut last: Option<String> = None;
    for r in &records {
        let line = line(r);
        if last.as_ref() != Some(&line) {
            let _ = writeln!(out, "{line}");
            last = Some(line);
        }
    }
    Ok(status::OK)
}

/// `<key, 16 hexadecimal digits> <kind> <declared|derived|tu> <flags> <name>`
/// (§8.9), the name written as §7.1 of `spec/verify.md` writes strings.
fn line(r: &Record) -> String {
    let class = match r.class {
        GroupClass::Declared => "declared",
        GroupClass::Derived => "derived",
        GroupClass::Tu => "tu",
    };
    let flags = match (r.indeterminate, r.key.is_none()) {
        (false, false) => "-",
        (true, false) => "i",
        (false, true) => "f",
        (true, true) => "if",
    };
    let key = r.id.to_string();
    format!("{} {} {class} {flags} {}", &key[..16], r.kind.name(), escape(r.name.as_bytes()))
}
