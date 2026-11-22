//! The shard, kind `SHRD` (`spec/model.md` §8).

use crate::json::Value;
use crate::wire::Encoder;
use crate::{Counter, Digest, Outcome, Shard, Status};

use super::common::{
    JResult, Result, dec_enum, dec_file_ref, dec_list, enc_enum, enc_file_ref, json_digest,
    json_enum, json_file_ref, json_uv, malformed, object, read_digest, read_enum, read_file_ref,
    read_list, read_record, read_str, read_uv,
};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, digests, open, read_container,
    render_container, seal, verdict,
};

const HEADER: u64 = 1;
const OUTCOME: u64 = 2;
const INPUTS: u64 = 3;
const COUNTERS: u64 = 4;
const DIGESTS: u64 = digests::ID;
const VERDICT: u64 = verdict::ID;

use super::MESSAGE_MAX;

// ---------------------------------------------------------------------------
// Invariants

#[derive(Clone, Copy, Debug)]
enum Loc {
    Message,
    Inputs,
    Input(usize),
    CounterName(usize),
    Counter(usize),
    Digests(digests::Loc),
    Verdict(verdict::Loc),
}

impl Loc {
    fn path(self) -> String {
        match self {
            Loc::Message => "sections.outcome.message".to_owned(),
            Loc::Inputs => "sections.inputs".to_owned(),
            Loc::Input(i) => format!("sections.inputs[{i}]"),
            Loc::CounterName(i) => format!("sections.counters[{i}].name"),
            Loc::Counter(i) => format!("sections.counters[{i}]"),
            Loc::Digests(l) => l.path(),
            Loc::Verdict(l) => l.path(),
        }
    }
}

/// `[a-z0-9_]+(\.[a-z0-9_]+)*` (§8).
fn is_counter_name(name: &str) -> bool {
    name.split('.').all(|word| {
        !word.is_empty()
            && word.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
    })
}

/// The first violation of the shard's invariants, in section order.
fn violation(s: &Shard) -> Option<(Loc, &'static str)> {
    let failed = s.outcome.status == Status::Failed;
    if s.outcome.status == Status::Ok && !s.outcome.message.is_empty() {
        return Some((Loc::Message, "message is not empty for status ok"));
    }
    if s.outcome.message.len() > MESSAGE_MAX {
        return Some((Loc::Message, "message is longer than 4096 bytes"));
    }
    if let Some(inputs) = &s.inputs {
        if failed {
            return Some((Loc::Inputs, "a failed shard has an inputs section"));
        }
        if let Some(i) = (1..inputs.len()).find(|&i| inputs[i - 1].path >= inputs[i].path) {
            return Some((Loc::Input(i), "inputs are not strictly ascending by path"));
        }
    }
    for (i, c) in s.counters.iter().enumerate() {
        if !is_counter_name(&c.name) {
            return Some((Loc::CounterName(i), "counter name is not [a-z0-9_]+(.[a-z0-9_]+)*"));
        }
        if i > 0 && s.counters[i - 1].name >= c.name {
            return Some((Loc::Counter(i), "counters are not strictly ascending by name"));
        }
    }
    if failed && !s.counters.is_empty() {
        return Some((Loc::Counter(0), "a failed shard has counters"));
    }
    if let Some(d) = &s.digests
        && let Some((l, what)) = digests::violation(d, s.outcome.status == Status::Ok)
    {
        return Some((Loc::Digests(l), what));
    }
    if let Some(v) = &s.verdict {
        let cx = verdict::Context {
            phase: s.phase,
            status: s.outcome.status,
            digests: s.digests.is_some(),
        };
        if let Some((l, what)) = verdict::violation(v, cx) {
            return Some((Loc::Verdict(l), what));
        }
    }
    None
}

pub(super) fn check(s: &Shard) -> std::result::Result<(), (String, &'static str)> {
    violation(s).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(s: &Shard, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut header = Encoder::new();
    header.digest(&s.observer);
    enc_enum(&mut header, s.phase);
    header.uv(s.round);
    let mut outcome = Encoder::new();
    enc_enum(&mut outcome, s.outcome.status);
    outcome.uv(s.outcome.errors).uv(s.outcome.warnings).str(&s.outcome.message);
    let inputs = s.inputs.as_ref().map(|inputs| {
        let mut e = Encoder::new();
        e.count(inputs.len());
        for f in inputs {
            enc_file_ref(&mut e, f);
        }
        e.into_bytes()
    });
    let mut counters = Encoder::new();
    counters.count(s.counters.len());
    for c in &s.counters {
        counters.str(&c.name).uv(c.value);
    }
    let mut sections = vec![(HEADER, header.as_bytes()), (OUTCOME, outcome.as_bytes())];
    if let Some(inputs) = &inputs {
        sections.push((INPUTS, inputs));
    }
    sections.push((COUNTERS, counters.as_bytes()));
    let digests = s.digests.as_ref().map(|d| {
        let mut e = Encoder::new();
        digests::encode(&mut e, d);
        e.into_bytes()
    });
    if let Some(digests) = &digests {
        sections.push((DIGESTS, digests));
    }
    let verdict = s.verdict.as_ref().map(|v| {
        let mut e = Encoder::new();
        verdict::encode(&mut e, v);
        e.into_bytes()
    });
    if let Some(verdict) = &verdict {
        sections.push((VERDICT, verdict));
    }
    seal(Shard::KIND, Shard::VERSION, &sections, extras)
}

/// The shard, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(Shard, Vec<Extra<'_>>, Digest)> {
    let opened = open(
        bytes,
        Shard::KIND,
        Shard::VERSION,
        [HEADER, OUTCOME, INPUTS, COUNTERS, DIGESTS, VERDICT],
    )?;
    let end = opened.end;
    let [header, outcome, inputs, counters, digests, verdict] = opened.sections;

    let mut d = super::required(header, end, "shard has no header section")?;
    let observer = d.digest()?;
    let phase = dec_enum(&mut d)?;
    let round = d.uv()?;
    d.finish()?;

    let mut d = super::required(outcome, end, "shard has no outcome section")?;
    let status = dec_enum(&mut d)?;
    let errors = d.uv()?;
    let warnings = d.uv()?;
    let message_at = d.offset();
    let message = d.str()?.to_owned();
    d.finish()?;

    let mut inputs_at = end;
    let mut input_offsets = Vec::new();
    let inputs = match inputs {
        None => None,
        Some(mut d) => {
            inputs_at = d.offset();
            let list = dec_list(&mut d, Some(&mut input_offsets), dec_file_ref)?;
            d.finish()?;
            Some(list)
        }
    };

    let mut d = super::required(counters, end, "shard has no counters section")?;
    let mut counter_offsets = Vec::new();
    let counters = dec_list(&mut d, Some(&mut counter_offsets), |d| {
        Ok(Counter { name: d.str()?.to_owned(), value: d.uv()? })
    })?;
    d.finish()?;

    let (digests, digests_at) = match digests {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = digests::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };
    let (verdict, verdict_at) = match verdict {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = verdict::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };

    let shard = Shard {
        observer,
        phase,
        round,
        outcome: Outcome { status, errors, warnings, message },
        inputs,
        counters,
        digests,
        verdict,
    };
    if let Some((l, what)) = violation(&shard) {
        let at = match l {
            Loc::Message => message_at,
            Loc::Inputs => inputs_at,
            Loc::Input(i) => input_offsets[i],
            Loc::CounterName(i) | Loc::Counter(i) => counter_offsets[i],
            Loc::Digests(l) => digests_at
                .as_ref()
                .map_or_else(|| unreachable!("a digests violation without digests"), |o| o.of(l)),
            Loc::Verdict(l) => verdict_at
                .as_ref()
                .map_or_else(|| unreachable!("a verdict violation without verdict"), |o| o.of(l)),
        };
        return Err(malformed(at, what));
    }
    Ok((shard, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 6] = [
    SectionName { id: HEADER, name: "header", required: true },
    SectionName { id: OUTCOME, name: "outcome", required: true },
    SectionName { id: INPUTS, name: "inputs", required: false },
    SectionName { id: COUNTERS, name: "counters", required: true },
    SectionName { id: DIGESTS, name: "digests", required: false },
    SectionName { id: VERDICT, name: "verdict", required: false },
];

pub(super) fn render(s: &Shard, extras: &[Extra<'_>]) -> Value {
    let mut sections = vec![
        (
            "header",
            object(vec![
                ("observer", json_digest(&s.observer)),
                ("phase", json_enum(s.phase)),
                ("round", json_uv(s.round)),
            ]),
        ),
        (
            "outcome",
            object(vec![
                ("status", json_enum(s.outcome.status)),
                ("errors", json_uv(s.outcome.errors)),
                ("warnings", json_uv(s.outcome.warnings)),
                ("message", Value::Str(s.outcome.message.clone())),
            ]),
        ),
    ];
    if let Some(inputs) = &s.inputs {
        sections.push(("inputs", Value::Array(inputs.iter().map(json_file_ref).collect())));
    }
    let counters = s
        .counters
        .iter()
        .map(|c| object(vec![("name", Value::Str(c.name.clone())), ("value", json_uv(c.value))]))
        .collect();
    sections.push(("counters", Value::Array(counters)));
    if let Some(d) = &s.digests {
        sections.push(("digests", digests::render(d)));
    }
    if let Some(v) = &s.verdict {
        sections.push(("verdict", verdict::render(v)));
    }
    render_container(Shard::KIND, Shard::VERSION, sections, extras)
}

/// The shard and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(Shard, OwnedExtras)> {
    let s = read_container(v, Shard::KIND, Shard::VERSION, &NAMES)?;
    let [Some(header), Some(outcome), inputs, Some(counters), digests, verdict] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");

    let at = sections.key("header");
    let [observer, phase, round] = read_record(header, &at, ["observer", "phase", "round"])?;
    let observer = read_digest(observer, &at.key("observer"))?;
    let phase = read_enum(phase, &at.key("phase"))?;
    let round = read_uv(round, &at.key("round"))?;

    let at = sections.key("outcome");
    let [status, errors, warnings, message] =
        read_record(outcome, &at, ["status", "errors", "warnings", "message"])?;
    let outcome = Outcome {
        status: read_enum(status, &at.key("status"))?,
        errors: read_uv(errors, &at.key("errors"))?,
        warnings: read_uv(warnings, &at.key("warnings"))?,
        message: read_str(message, &at.key("message"))?.to_owned(),
    };

    let inputs =
        inputs.map(|v| read_list(v, &sections.key("inputs"), read_file_ref)).transpose()?;
    let counters = read_list(counters, &sections.key("counters"), |v, at| {
        let [name, value] = read_record(v, at, ["name", "value"])?;
        Ok(Counter {
            name: read_str(name, &at.key("name"))?.to_owned(),
            value: read_uv(value, &at.key("value"))?,
        })
    })?;

    let digests = digests.map(|v| digests::read(v, &sections.key("digests"))).transpose()?;
    let verdict = verdict.map(|v| verdict::read(v, &sections.key("verdict"))).transpose()?;

    let shard = Shard { observer, phase, round, outcome, inputs, counters, digests, verdict };
    if let Some((l, what)) = violation(&shard) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((shard, s.extras))
}
