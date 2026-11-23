//! The `names` section, id 27 of a shard of phase `map` or `verify`
//! (`spec/model.md` §8.6). Work package WP-112; the order of checks,
//! offsets and paths are `spec/landed/verify/WP-112.md` §2.

use crate::json::Value;
use crate::names::{NameNote, Names, NamesEntry};
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::MESSAGE_MAX;
use super::common::{
    JResult, Result, dec_list, json_bytes, json_digest, object, read_bytes, read_digest, read_list,
    read_record, read_str,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 27;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Unit(usize),
    Note(usize),
    NoteName(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.names";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Unit(i) => format!("{S}.units[{i}]"),
            Loc::Note(i) => format!("{S}.notes[{i}]"),
            Loc::NoteName(i) => format!("{S}.notes[{i}].name"),
        }
    }
}

/// The shard's facts the section's placement depends on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Context {
    pub phase: Phase,
    pub status: Status,
    /// The shard has a `digests` section.
    pub digests: bool,
    /// The shard has a verdict of kind `verified` or `mismatch`.
    pub compared: bool,
}

/// The first violation, each rule over its whole list before the next
/// (`spec/model.md` §8.6, rows 2 to 7; row 1, the section's presence, is
/// the shard's): status `ok`; a `digests` section; `units` strictly
/// ascending by key; no note in a shard of phase `map`, nor in one whose
/// verdict is not `verified` or `mismatch`; `notes` strictly ascending by
/// key; every note's key in `units`; every note's name at most 4096
/// bytes.
pub(super) fn violation(n: &Names, cx: Context) -> Option<(Loc, &'static str)> {
    if cx.status != Status::Ok {
        return Some((Loc::Section, "a names section in a shard whose status is not ok"));
    }
    if !cx.digests {
        return Some((Loc::Section, "a names section in a shard without a digests section"));
    }
    if let Some(i) = (1..n.units.len()).find(|&i| n.units[i - 1].key >= n.units[i].key) {
        return Some((Loc::Unit(i), "units are not strictly ascending by key"));
    }
    if !n.notes.is_empty() {
        if cx.phase == Phase::Map {
            return Some((Loc::Note(0), "a note in a shard of phase map"));
        }
        if !cx.compared {
            return Some((
                Loc::Note(0),
                "a note in a shard whose verdict is not verified or mismatch",
            ));
        }
    }
    if let Some(i) = (1..n.notes.len()).find(|&i| n.notes[i - 1].key >= n.notes[i].key) {
        return Some((Loc::Note(i), "notes are not strictly ascending by key"));
    }
    if let Some(i) =
        n.notes.iter().position(|x| n.units.binary_search_by(|u| u.key.cmp(&x.key)).is_err())
    {
        return Some((Loc::Note(i), "a note's key has no entry in units"));
    }
    if let Some(i) = n.notes.iter().position(|x| x.name.len() > MESSAGE_MAX) {
        return Some((Loc::NoteName(i), "longer than 4096 bytes"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(e: &mut Encoder, n: &Names) {
    e.count(n.units.len());
    for u in &n.units {
        e.digest(&u.key).digest(&u.names);
    }
    e.count(n.notes.len());
    for x in &n.notes {
        e.digest(&x.key).bytes(&x.file).str(&x.name);
    }
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    units: Vec<usize>,
    notes: Vec<usize>,
    names: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::Unit(i) => self.units[i],
            Loc::Note(i) => self.notes[i],
            Loc::NoteName(i) => self.names[i],
        }
    }
}

pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Names, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let units = dec_list(d, Some(&mut o.units), |d| {
        Ok(NamesEntry { key: d.digest()?, names: d.digest()? })
    })?;
    let notes = dec_list(d, Some(&mut o.notes), |d| {
        let key = d.digest()?;
        let file = d.bytes()?.to_vec();
        o.names.push(d.offset());
        Ok(NameNote { key, file, name: d.str()?.to_owned() })
    })?;
    Ok((Names { units, notes }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render(n: &Names) -> Value {
    let units = n
        .units
        .iter()
        .map(|u| object(vec![("key", json_digest(&u.key)), ("names", json_digest(&u.names))]))
        .collect();
    let notes = n
        .notes
        .iter()
        .map(|x| {
            object(vec![
                ("key", json_digest(&x.key)),
                ("file", json_bytes(&x.file)),
                ("name", Value::Str(x.name.clone())),
            ])
        })
        .collect();
    object(vec![("units", Value::Array(units)), ("notes", Value::Array(notes))])
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Names> {
    let [units, notes] = read_record(v, at, ["units", "notes"])?;
    Ok(Names {
        units: read_list(units, &at.key("units"), |v, at| {
            let [key, names] = read_record(v, at, ["key", "names"])?;
            Ok(NamesEntry {
                key: read_digest(key, &at.key("key"))?,
                names: read_digest(names, &at.key("names"))?,
            })
        })?,
        notes: read_list(notes, &at.key("notes"), |v, at| {
            let [key, file, name] = read_record(v, at, ["key", "file", "name"])?;
            Ok(NameNote {
                key: read_digest(key, &at.key("key"))?,
                file: read_bytes(file, &at.key("file"))?,
                name: read_str(name, &at.key("name"))?.to_owned(),
            })
        })?,
    })
}
