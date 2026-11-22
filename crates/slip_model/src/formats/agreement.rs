//! The `binding` and `rule_use` sections, ids 24 and 25 of a verify shard
//! (`spec/verify.md` §9.1). Work package WP-060; the order of checks,
//! offsets, paths and the JSON form are `spec/landed/edits/WP-060.md` §3.

use crate::agreement::{Binding, RuleUse, Use};
use crate::json::Value;
use crate::wire::{Decoder, Encoder};
use crate::{Phase, Status};

use super::At;
use super::common::{
    JResult, Result, dec_list, dec_opt, json_digest, json_opt, json_uv, object, read_digest,
    read_list, read_opt, read_record, read_uv,
};

/// The section ids (`spec/model.md` §8).
pub(super) const BINDING: u64 = 24;
pub(super) const RULE_USE: u64 = 25;

/// Where an invariant is violated.
#[derive(Clone, Copy, Debug)]
pub(super) enum Loc {
    Binding,
    RuleUse,
    Use(usize),
}

impl Loc {
    pub(super) fn path(self) -> String {
        match self {
            Loc::Binding => "sections.binding".to_owned(),
            Loc::RuleUse => "sections.rule_use".to_owned(),
            Loc::Use(i) => format!("sections.rule_use.uses[{i}]"),
        }
    }
}

/// `binding` (`spec/model.md` §8): phase `verify`, any status but `failed`.
pub(super) fn binding_violation(phase: Phase, status: Status) -> Option<(Loc, &'static str)> {
    if phase != Phase::Verify {
        return Some((Loc::Binding, "a binding section in a shard whose phase is not verify"));
    }
    if status == Status::Failed {
        return Some((Loc::Binding, "a binding section in a shard whose status is failed"));
    }
    None
}

/// `rule_use` (`spec/model.md` §8, `spec/verify.md` §9.1): phase `verify`,
/// status `ok`; then `uses` strictly ascending by `rule` over the whole
/// list, then no `Use` with both counts 0.
pub(super) fn rule_use_violation(
    r: &RuleUse,
    phase: Phase,
    status: Status,
) -> Option<(Loc, &'static str)> {
    if phase != Phase::Verify {
        return Some((Loc::RuleUse, "a rule_use section in a shard whose phase is not verify"));
    }
    if status != Status::Ok {
        return Some((Loc::RuleUse, "a rule_use section in a shard whose status is not ok"));
    }
    let uses = &r.uses;
    if let Some(i) = (1..uses.len()).find(|&i| uses[i - 1].rule >= uses[i].rule) {
        return Some((Loc::Use(i), "uses are not strictly ascending by rule"));
    }
    if let Some(i) = uses.iter().position(|u| u.via == 0 && u.direct == 0) {
        return Some((Loc::Use(i), "a use has via and direct both 0"));
    }
    None
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode_binding(e: &mut Encoder, b: &Binding) {
    match &b.overlay {
        None => e.u8(0),
        Some(d) => e.u8(1).digest(d),
    };
    e.digest(&b.declared).uv(b.pre);
}

pub(super) fn encode_rule_use(e: &mut Encoder, r: &RuleUse) {
    e.digest(&r.rules).count(r.uses.len());
    for u in &r.uses {
        e.uv(u.rule).uv(u.via).uv(u.direct);
    }
}

/// The payload and the offset of its first byte.
pub(super) fn decode_binding(d: &mut Decoder<'_>) -> Result<(Binding, usize)> {
    let at = d.offset();
    let overlay = dec_opt(d, Decoder::digest)?;
    Ok((Binding { overlay, declared: d.digest()?, pre: d.uv()? }, at))
}

/// File offsets of what an invariant of `rule_use` can name.
#[derive(Debug, Default)]
pub(super) struct Offsets {
    section: usize,
    uses: Vec<usize>,
}

impl Offsets {
    pub(super) fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Binding | Loc::RuleUse => self.section,
            Loc::Use(i) => self.uses[i],
        }
    }
}

pub(super) fn decode_rule_use(d: &mut Decoder<'_>) -> Result<(RuleUse, Offsets)> {
    let mut o = Offsets { section: d.offset(), ..Offsets::default() };
    let rules = d.digest()?;
    let uses = dec_list(d, Some(&mut o.uses), |d| {
        Ok(Use { rule: d.uv()?, via: d.uv()?, direct: d.uv()? })
    })?;
    Ok((RuleUse { rules, uses }, o))
}

// ---------------------------------------------------------------------------
// JSON

pub(super) fn render_binding(b: &Binding) -> Value {
    object(vec![
        ("overlay", json_opt(b.overlay.as_ref(), json_digest)),
        ("declared", json_digest(&b.declared)),
        ("pre", json_uv(b.pre)),
    ])
}

pub(super) fn render_rule_use(r: &RuleUse) -> Value {
    let uses = r
        .uses
        .iter()
        .map(|u| {
            object(vec![
                ("rule", json_uv(u.rule)),
                ("via", json_uv(u.via)),
                ("direct", json_uv(u.direct)),
            ])
        })
        .collect();
    object(vec![("rules", json_digest(&r.rules)), ("uses", Value::Array(uses))])
}

pub(super) fn read_binding(v: &Value, at: &At<'_>) -> JResult<Binding> {
    let [overlay, declared, pre] = read_record(v, at, ["overlay", "declared", "pre"])?;
    Ok(Binding {
        overlay: read_opt(overlay, &at.key("overlay"), read_digest)?,
        declared: read_digest(declared, &at.key("declared"))?,
        pre: read_uv(pre, &at.key("pre"))?,
    })
}

pub(super) fn read_rule_use(v: &Value, at: &At<'_>) -> JResult<RuleUse> {
    let [rules, uses] = read_record(v, at, ["rules", "uses"])?;
    Ok(RuleUse {
        rules: read_digest(rules, &at.key("rules"))?,
        uses: read_list(uses, &at.key("uses"), |v, at| {
            let [rule, via, direct] = read_record(v, at, ["rule", "via", "direct"])?;
            Ok(Use {
                rule: read_uv(rule, &at.key("rule"))?,
                via: read_uv(via, &at.key("via"))?,
                direct: read_uv(direct, &at.key("direct"))?,
            })
        })?,
    })
}
