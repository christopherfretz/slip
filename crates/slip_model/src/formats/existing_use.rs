//! The `existing_use` section, id 34 of a verify shard (`spec/verify.md`
//! §9.1; work package WP-144).
//!
//! ```text
//! ExistingUse := existing: list<uv>    indices into Declared.existing; strictly ascending
//!                local:    list<uv>    indices into Declared.existing_local; strictly ascending
//! ```
//!
//! An index out of range for the run's `Declared` cannot be seen here:
//! the fold of `slip_verify::run` checks it.

use crate::agreement::ExistingUse;
use crate::json::Value;
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::common::{JResult, Result, dec_list, json_uv, object, read_list, read_record, read_uv};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 34;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Existing(usize),
    Local(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.existing_use";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Existing(i) => format!("{S}.existing[{i}]"),
            Loc::Local(i) => format!("{S}.local[{i}]"),
        }
    }
}

/// The first violation, in order: `existing` strictly ascending; `local`
/// strictly ascending; not empty; phase `verify`; status `ok`
/// (`spec/model.md` §8, as for `rule_use` and `rebuilt_use`).
pub(super) fn violation(
    e: &ExistingUse,
    phase: Phase,
    status: Status,
) -> Option<(Loc, &'static str)> {
    if let Some(i) = (1..e.existing.len()).find(|&i| e.existing[i - 1] >= e.existing[i]) {
        return Some((Loc::Existing(i), "existing is not strictly ascending"));
    }
    if let Some(i) = (1..e.local.len()).find(|&i| e.local[i - 1] >= e.local[i]) {
        return Some((Loc::Local(i), "local is not strictly ascending"));
    }
    if e.is_empty() {
        return Some((Loc::Section, "an empty existing_use"));
    }
    if phase != Phase::Verify {
        return Some((
            Loc::Section,
            "an existing_use section in a shard whose phase is not verify",
        ));
    }
    if status != Status::Ok {
        return Some((Loc::Section, "an existing_use section in a shard whose status is not ok"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(enc: &mut Encoder, x: &ExistingUse) {
    enc.count(x.existing.len());
    for i in &x.existing {
        enc.uv(*i);
    }
    enc.count(x.local.len());
    for i in &x.local {
        enc.uv(*i);
    }
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    existing: Vec<usize>,
    local: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::Existing(i) => self.existing[i],
            Loc::Local(i) => self.local[i],
        }
    }
}

pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(ExistingUse, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let existing = dec_list(d, Some(&mut o.existing), Decoder::uv)?;
    let local = dec_list(d, Some(&mut o.local), Decoder::uv)?;
    Ok((ExistingUse { existing, local }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render(x: &ExistingUse) -> Value {
    object(vec![
        ("existing", Value::Array(x.existing.iter().map(|&i| json_uv(i)).collect())),
        ("local", Value::Array(x.local.iter().map(|&i| json_uv(i)).collect())),
    ])
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<ExistingUse> {
    let [existing, local] = read_record(v, at, ["existing", "local"])?;
    Ok(ExistingUse {
        existing: read_list(existing, &at.key("existing"), read_uv)?,
        local: read_list(local, &at.key("local"), read_uv)?,
    })
}
