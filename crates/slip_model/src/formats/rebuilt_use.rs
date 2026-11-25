//! The `rebuilt_use` section, id 33 of a verify shard (`spec/verify.md`
//! §9.1; work package WP-118, `spec/landed/verify/WP-118.md` §4.4).
//!
//! ```text
//! RebuiltUse := relied:     list<uv>          indices into Declared.files.rebuilt; strictly ascending
//!               declares:   list<Declares>    strictly ascending by (file, rule)
//!               hints:      list<path>        strictly ascending; at most 16
//!               hints_more: uv                0 unless `hints` has 16
//! Declares   := file: path, rule: uv, mapped: uv, left: uv, left_kept: uv
//! ```
//!
//! An index out of range for the run's `Declared` cannot be seen here:
//! the fold of `slip_verify::run` checks it.

use crate::agreement::{Declares, RebuiltUse};
use crate::json::Value;
use crate::transform::is_file_path;
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::common::{
    JResult, Result, dec_list, json_bytes, json_uv, object, read_bytes, read_list, read_record,
    read_uv,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 33;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Relied(usize),
    Declares(usize),
    Hints,
    Hint(usize),
    HintsMore,
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.rebuilt_use";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Relied(i) => format!("{S}.relied[{i}]"),
            Loc::Declares(i) => format!("{S}.declares[{i}]"),
            Loc::Hints => format!("{S}.hints"),
            Loc::Hint(i) => format!("{S}.hints[{i}]"),
            Loc::HintsMore => format!("{S}.hints_more"),
        }
    }
}

const NOT_CANONICAL: &str = "not a canonical absolute path";

/// The first violation (WP-118 §4.4), in this order: `relied` strictly
/// ascending; `declares` strictly ascending by file, then rule; no row
/// that counts nothing; `hints` strictly ascending; at most 16;
/// `hints_more` only with 16; every path canonical; the section not
/// empty; then the shard's phase `verify` and status `ok`
/// (`spec/model.md` §8, as for `rule_use`).
pub(super) fn violation(
    r: &RebuiltUse,
    phase: Phase,
    status: Status,
) -> Option<(Loc, &'static str)> {
    if let Some(i) = (1..r.relied.len()).find(|&i| r.relied[i - 1] >= r.relied[i]) {
        return Some((Loc::Relied(i), "relied is not strictly ascending"));
    }
    let d = &r.declares;
    if let Some(i) =
        (1..d.len()).find(|&i| (&d[i - 1].file, d[i - 1].rule) >= (&d[i].file, d[i].rule))
    {
        return Some((Loc::Declares(i), "declares is not strictly ascending"));
    }
    if let Some(i) = d.iter().position(|x| x.mapped == 0 && x.left == 0 && x.left_kept == 0) {
        return Some((Loc::Declares(i), "a declares row counts nothing"));
    }
    if let Some(i) = (1..r.hints.len()).find(|&i| r.hints[i - 1] >= r.hints[i]) {
        return Some((Loc::Hint(i), "hints is not strictly ascending"));
    }
    if r.hints.len() > RebuiltUse::HINTS_MAX {
        return Some((Loc::Hints, "more than 16 hints"));
    }
    if r.hints_more != 0 && r.hints.len() != RebuiltUse::HINTS_MAX {
        return Some((Loc::HintsMore, "hints_more without 16 hints"));
    }
    if let Some(i) = d.iter().position(|x| !is_file_path(&x.file)) {
        return Some((Loc::Declares(i), NOT_CANONICAL));
    }
    if let Some(i) = r.hints.iter().position(|h| !is_file_path(h)) {
        return Some((Loc::Hint(i), NOT_CANONICAL));
    }
    if r.is_empty() {
        return Some((Loc::Section, "an empty rebuilt_use"));
    }
    if phase != Phase::Verify {
        return Some((Loc::Section, "a rebuilt_use section in a shard whose phase is not verify"));
    }
    if status != Status::Ok {
        return Some((Loc::Section, "a rebuilt_use section in a shard whose status is not ok"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(e: &mut Encoder, r: &RebuiltUse) {
    e.count(r.relied.len());
    for i in &r.relied {
        e.uv(*i);
    }
    e.count(r.declares.len());
    for x in &r.declares {
        e.bytes(&x.file).uv(x.rule).uv(x.mapped).uv(x.left).uv(x.left_kept);
    }
    e.count(r.hints.len());
    for h in &r.hints {
        e.bytes(h);
    }
    e.uv(r.hints_more);
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    relied: Vec<usize>,
    declares: Vec<usize>,
    hints: usize,
    hint: Vec<usize>,
    hints_more: usize,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section => self.section,
            Loc::Relied(i) => self.relied[i],
            Loc::Declares(i) => self.declares[i],
            Loc::Hints => self.hints,
            Loc::Hint(i) => self.hint[i],
            Loc::HintsMore => self.hints_more,
        }
    }
}

pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(RebuiltUse, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let relied = dec_list(d, Some(&mut o.relied), Decoder::uv)?;
    let declares = dec_list(d, Some(&mut o.declares), |d| {
        Ok(Declares {
            file: d.bytes()?.to_vec(),
            rule: d.uv()?,
            mapped: d.uv()?,
            left: d.uv()?,
            left_kept: d.uv()?,
        })
    })?;
    o.hints = d.offset();
    let hints = dec_list(d, Some(&mut o.hint), |d| Ok(d.bytes()?.to_vec()))?;
    o.hints_more = d.offset();
    let hints_more = d.uv()?;
    Ok((RebuiltUse { relied, declares, hints, hints_more }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render(r: &RebuiltUse) -> Value {
    let declares = r
        .declares
        .iter()
        .map(|x| {
            object(vec![
                ("file", json_bytes(&x.file)),
                ("rule", json_uv(x.rule)),
                ("mapped", json_uv(x.mapped)),
                ("left", json_uv(x.left)),
                ("left_kept", json_uv(x.left_kept)),
            ])
        })
        .collect();
    object(vec![
        ("relied", Value::Array(r.relied.iter().map(|&i| json_uv(i)).collect())),
        ("declares", Value::Array(declares)),
        ("hints", Value::Array(r.hints.iter().map(|h| json_bytes(h)).collect())),
        ("hints_more", json_uv(r.hints_more)),
    ])
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<RebuiltUse> {
    let [relied, declares, hints, hints_more] =
        read_record(v, at, ["relied", "declares", "hints", "hints_more"])?;
    Ok(RebuiltUse {
        relied: read_list(relied, &at.key("relied"), read_uv)?,
        declares: read_list(declares, &at.key("declares"), |v, at| {
            let [file, rule, mapped, left, left_kept] =
                read_record(v, at, ["file", "rule", "mapped", "left", "left_kept"])?;
            Ok(Declares {
                file: read_bytes(file, &at.key("file"))?,
                rule: read_uv(rule, &at.key("rule"))?,
                mapped: read_uv(mapped, &at.key("mapped"))?,
                left: read_uv(left, &at.key("left"))?,
                left_kept: read_uv(left_kept, &at.key("left_kept"))?,
            })
        })?,
        hints: read_list(hints, &at.key("hints"), read_bytes)?,
        hints_more: read_uv(hints_more, &at.key("hints_more"))?,
    })
}
