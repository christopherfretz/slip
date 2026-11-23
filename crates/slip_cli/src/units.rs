//! `slip units` (`spec/exec.md` §8.9): the unit records of the selected
//! observers, for a person writing a declared file or reading a report.
//! Work package WP-053. The selection, its bound and the in-process parses
//! are `declare`'s (`crate::declare::InProcess`).

use std::io::Write;
use std::process::ExitCode;

use slip_digest::records::Record;
use slip_model::Digest;
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
    let records: Vec<Record> = merged
        .records
        .into_iter()
        .filter(|r| {
            a.matches.is_empty() || a.matches.iter().any(|m| contains(r.name.as_bytes(), m))
        })
        .collect();
    for line in lines(&records) {
        let _ = writeln!(out, "{line}");
    }
    Ok(status::OK)
}

/// The lines of `records`, sorted by `(name, key)`, then by the line, so
/// that equal lines are adjacent: records of one `KeyId` can differ in what
/// the line shows (an `i` from one observer, `-` from another) and would
/// otherwise interleave. Each line once.
fn lines(records: &[Record]) -> Vec<String> {
    let mut lines: Vec<(&str, Digest, String)> =
        records.iter().map(|r| (r.name.as_str(), r.id, line(r))).collect();
    lines.sort();
    lines.dedup();
    lines.into_iter().map(|(_, _, line)| line).collect()
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

#[cfg(test)]
mod tests {
    use slip_model::keys::KeyKind;

    use super::*;

    fn record(id: u8, name: &str, indeterminate: bool) -> Record {
        Record {
            id: Digest([id; 32]),
            key: None,
            kind: KeyKind::Function,
            class: GroupClass::Declared,
            name: name.to_owned(),
            indeterminate,
        }
    }

    // §8.9 (review F4): records of one id with different flags, merged from
    // several observers in an order that interleaves them under `(name,
    // id)`: each line is printed once, the equal lines adjacent.
    #[test]
    fn s8_9_equal_lines_once() {
        let records = [
            record(1, "f()", false),
            record(1, "f()", true),
            record(1, "f()", false),
            record(2, "f()", false),
            record(1, "f()", true),
        ];
        let got = lines(&records);
        assert_eq!(got.len(), 3, "{got:?}");
        let mut unique = got.clone();
        unique.dedup();
        assert_eq!(unique, got);
        assert!(got[0].starts_with("0101010101010101 ") && got[2].starts_with("0202020202020202 "));
    }
}
