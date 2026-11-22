//! The `spellings` section, id 26 of a verify shard (`spec/verify.md`
//! §10.4). Work package WP-060; the order of checks, offsets, paths and the
//! JSON form are `spec/landed/edits/WP-060.md` §4.

use crate::json::Value;
use crate::spellings::{Failed, Span, Spellings};
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::common::{
    JResult, Result, dec_list, dec_opt, json_digest, json_opt, json_uv, object, read_digest,
    read_list, read_opt, read_record, read_str, read_uv,
};

/// The section id (`spec/model.md` §8).
pub(super) const ID: u64 = 26;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Section,
    Seen,
    Held(usize),
    HeldCount(usize),
    Failed(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        const S: &str = "sections.spellings";
        match self {
            Loc::Section => S.to_owned(),
            Loc::Seen => format!("{S}.seen"),
            Loc::Held(i) => format!("{S}.held[{i}]"),
            Loc::HeldCount(i) => format!("{S}.held[{i}].count"),
            Loc::Failed(i) => format!("{S}.failed[{i}]"),
        }
    }
}

/// One past the last index of `s`, if it is below 2^64.
fn end(s: &Span) -> Option<u64> {
    s.first.checked_add(s.count)
}

/// The first violation, each rule over its whole list before the next:
/// phase and status; every span's count at least 1 and its end at most
/// 2^64; spans ascending and no two touching (so no index is in `held`
/// twice); `failed` strictly ascending by `index`; no failed index in
/// `held`; at most `seen` indices in the two lists.
pub(super) fn violation(
    s: &Spellings,
    phase: Phase,
    status: Status,
) -> Option<(Loc, &'static str)> {
    if phase != Phase::Verify {
        return Some((Loc::Section, "a spellings section in a shard whose phase is not verify"));
    }
    if status != Status::Ok {
        return Some((Loc::Section, "a spellings section in a shard whose status is not ok"));
    }
    if let Some(i) = s.held.iter().position(|h| h.count == 0) {
        return Some((Loc::HeldCount(i), "a span has a count of 0"));
    }
    if let Some(i) = s.held.iter().position(|h| h.first.checked_add(h.count - 1).is_none()) {
        return Some((Loc::HeldCount(i), "a span runs past the largest index"));
    }
    // A span whose end is 2^64 is the last one possible: any span after it
    // starts below its end.
    if let Some(i) =
        (1..s.held.len()).find(|&i| end(&s.held[i - 1]).is_none_or(|e| e >= s.held[i].first))
    {
        return Some((Loc::Held(i), "spans are not ascending or touch"));
    }
    if let Some(i) = (1..s.failed.len()).find(|&i| s.failed[i - 1].index >= s.failed[i].index) {
        return Some((Loc::Failed(i), "failed is not strictly ascending by index"));
    }
    let in_held = |x: u64| {
        let k = s.held.partition_point(|h| h.first <= x);
        k > 0 && end(&s.held[k - 1]).is_none_or(|e| x < e)
    };
    if let Some(i) = s.failed.iter().position(|f| in_held(f.index)) {
        return Some((Loc::Failed(i), "an index is both held and failed"));
    }
    let total: u128 =
        s.held.iter().map(|h| u128::from(h.count)).sum::<u128>() + s.failed.len() as u128;
    if total > u128::from(s.seen) {
        return Some((Loc::Seen, "more obligations held or failed than seen"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(e: &mut Encoder, s: &Spellings) {
    e.uv(s.seen).count(s.held.len());
    for h in &s.held {
        e.uv(h.first).uv(h.count);
    }
    e.count(s.failed.len());
    for f in &s.failed {
        e.uv(f.index);
        match &f.found {
            None => e.u8(0),
            Some(k) => e.u8(1).digest(k),
        };
        e.str(&f.name);
    }
}

/// File offsets of what an invariant can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    seen: usize,
    held: Vec<usize>,
    count: Vec<usize>,
    failed: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Section | Loc::Seen => self.seen,
            Loc::Held(i) => self.held[i],
            Loc::HeldCount(i) => self.count[i],
            Loc::Failed(i) => self.failed[i],
        }
    }
}

pub(super) fn decode(d: &mut Decoder<'_>) -> Result<(Spellings, Offsets)> {
    let mut o = Offsets { seen: d.offset(), ..Offsets::default() };
    let seen = d.uv()?;
    let held = dec_list(d, Some(&mut o.held), |d| {
        let first = d.uv()?;
        o.count.push(d.offset());
        Ok(Span { first, count: d.uv()? })
    })?;
    let failed = dec_list(d, Some(&mut o.failed), |d| {
        Ok(Failed {
            index: d.uv()?,
            found: dec_opt(d, Decoder::digest)?,
            name: d.str()?.to_owned(),
        })
    })?;
    Ok((Spellings { seen, held, failed }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render(s: &Spellings) -> Value {
    let held = s
        .held
        .iter()
        .map(|h| object(vec![("first", json_uv(h.first)), ("count", json_uv(h.count))]))
        .collect();
    let failed = s
        .failed
        .iter()
        .map(|f| {
            object(vec![
                ("index", json_uv(f.index)),
                ("found", json_opt(f.found.as_ref(), json_digest)),
                ("name", Value::Str(f.name.clone())),
            ])
        })
        .collect();
    object(vec![
        ("seen", json_uv(s.seen)),
        ("held", Value::Array(held)),
        ("failed", Value::Array(failed)),
    ])
}

pub(super) fn read(v: &Value, at: &At<'_>) -> JResult<Spellings> {
    let [seen, held, failed] = read_record(v, at, ["seen", "held", "failed"])?;
    Ok(Spellings {
        seen: read_uv(seen, &at.key("seen"))?,
        held: read_list(held, &at.key("held"), |v, at| {
            let [first, count] = read_record(v, at, ["first", "count"])?;
            Ok(Span {
                first: read_uv(first, &at.key("first"))?,
                count: read_uv(count, &at.key("count"))?,
            })
        })?,
        failed: read_list(failed, &at.key("failed"), |v, at| {
            let [index, found, name] = read_record(v, at, ["index", "found", "name"])?;
            Ok(Failed {
                index: read_uv(index, &at.key("index"))?,
                found: read_opt(found, &at.key("found"), read_digest)?,
                name: read_str(name, &at.key("name"))?.to_owned(),
            })
        })?,
    })
}
