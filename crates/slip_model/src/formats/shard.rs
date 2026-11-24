//! The shard, kind `SHRD` (`spec/model.md` §8), with the sections of M2
//! (`edits`, `binding`, `rule_use`, `spellings`; work package WP-060) and
//! `negatives` (work package WP-117).

use crate::json::Value;
use crate::verdict::VerdictKind;
use crate::wire::Encoder;
use crate::{Counter, Digest, Outcome, Shard, Status};

use super::common::{
    JResult, Result, dec_enum, dec_file_ref, dec_list, enc_enum, enc_file_ref, json_digest,
    json_enum, json_file_ref, json_uv, malformed, object, read_digest, read_enum, read_file_ref,
    read_list, read_record, read_str, read_uv,
};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, agreement, digests, edits, names,
    negatives, open, read_container, render_container, seal, spellings, verdict,
};

const HEADER: u64 = 1;
const OUTCOME: u64 = 2;
const INPUTS: u64 = 3;
const COUNTERS: u64 = 4;
const EDITS: u64 = edits::ID;
const DIGESTS: u64 = digests::ID;
const VERDICT: u64 = verdict::ID;
const BINDING: u64 = agreement::BINDING;
const RULE_USE: u64 = agreement::RULE_USE;
const SPELLINGS: u64 = spellings::ID;
const NAMES: u64 = names::ID;
const NEGATIVES: u64 = negatives::ID;

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
    Edits(edits::Loc),
    Agreement(agreement::Loc),
    Spellings(spellings::Loc),
    Names(names::Loc),
    /// A shard with `digests` and no `names`: at the `digests` payload.
    NoNames,
    Negatives(negatives::Loc),
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
            Loc::Edits(l) => l.path(),
            Loc::Agreement(l) => l.path(),
            Loc::Spellings(l) => l.path(),
            Loc::Names(l) => l.path(),
            Loc::NoNames => "sections.names".to_owned(),
            Loc::Negatives(l) => l.path(),
        }
    }
}

/// `[a-z0-9_]+(\.[a-z0-9_]+)*` (§8).
pub(super) fn is_counter_name(name: &str) -> bool {
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
    // spec/model.md §8: the sections of M2, in the order of their ids.
    let (phase, status) = (s.phase, s.outcome.status);
    if let Some(e) = &s.edits
        && let Some((l, what)) = edits::violation(e, edits::Context { phase, status })
    {
        return Some((Loc::Edits(l), what));
    }
    if s.binding.is_some()
        && let Some((l, what)) = agreement::binding_violation(phase, status)
    {
        return Some((Loc::Agreement(l), what));
    }
    if let Some(x) = &s.spellings
        && let Some((l, what)) = spellings::violation(x, phase, status)
    {
        return Some((Loc::Spellings(l), what));
    }
    // spec/model.md §8.7: `names` with `digests`, and only with it.
    match &s.names {
        None if s.digests.is_some() => {
            return Some((Loc::NoNames, "a shard with a digests section has no names section"));
        }
        None => {}
        Some(n) => {
            let compared = s
                .verdict
                .as_ref()
                .is_some_and(|v| matches!(v.kind, VerdictKind::Verified | VerdictKind::Mismatch));
            let cx = names::Context { phase, status, digests: s.digests.is_some(), compared };
            if let Some((l, what)) = names::violation(n, cx) {
                return Some((Loc::Names(l), what));
            }
        }
    }
    if let Some(r) = &s.rule_use
        && let Some((l, what)) = agreement::rule_use_violation(r, phase, status)
    {
        return Some((Loc::Agreement(l), what));
    }
    if let Some(n) = &s.negatives
        && let Some((l, what)) = negatives::violation(n, phase, s.inputs.is_some())
    {
        return Some((Loc::Negatives(l), what));
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
    let edits = s.edits.as_ref().map(|x| {
        let mut e = Encoder::new();
        edits::encode(&mut e, x);
        e.into_bytes()
    });
    if let Some(edits) = &edits {
        sections.push((EDITS, edits));
    }
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
    let binding = s.binding.as_ref().map(|b| {
        let mut e = Encoder::new();
        agreement::encode_binding(&mut e, b);
        e.into_bytes()
    });
    if let Some(binding) = &binding {
        sections.push((BINDING, binding));
    }
    let spellings = s.spellings.as_ref().map(|x| {
        let mut e = Encoder::new();
        spellings::encode(&mut e, x);
        e.into_bytes()
    });
    if let Some(spellings) = &spellings {
        sections.push((SPELLINGS, spellings));
    }
    let names = s.names.as_ref().map(|x| {
        let mut e = Encoder::new();
        names::encode(&mut e, x);
        e.into_bytes()
    });
    if let Some(names) = &names {
        sections.push((NAMES, names));
    }
    let negatives = s.negatives.as_ref().map(|x| {
        let mut e = Encoder::new();
        negatives::encode(&mut e, x);
        e.into_bytes()
    });
    if let Some(negatives) = &negatives {
        sections.push((NEGATIVES, negatives));
    }
    let rule_use = s.rule_use.as_ref().map(|r| {
        let mut e = Encoder::new();
        agreement::encode_rule_use(&mut e, r);
        e.into_bytes()
    });
    if let Some(rule_use) = &rule_use {
        sections.push((RULE_USE, rule_use));
    }
    seal(Shard::KIND, Shard::VERSION, &sections, extras)
}

/// The first version this reader reads (`model.md` §8): a version-1
/// shard predates the `names` section (WP-112), and one without it is
/// not "a shard without notes"; a store written before is parsed again.
const FIRST_READ: u32 = 2;

/// Why a shard older than [`FIRST_READ`] is refused.
const OLDER: &str = "shard version older than this reader reads (it predates the names \
                     section; parse again)";

/// The shard, its unknown ignorable sections and its content id.
#[allow(clippy::too_many_lines, reason = "one block per section of spec/model.md §8")]
pub(super) fn decode(bytes: &[u8]) -> Result<(Shard, Vec<Extra<'_>>, Digest)> {
    let opened = open(
        bytes,
        Shard::KIND,
        Shard::VERSION,
        [
            HEADER, OUTCOME, INPUTS, COUNTERS, EDITS, DIGESTS, VERDICT, BINDING, SPELLINGS, NAMES,
            NEGATIVES, RULE_USE,
        ],
    )?;
    if opened.version < FIRST_READ {
        return Err(crate::wire::Error::new(crate::wire::ErrorKind::UnsupportedVersion, 12, OLDER));
    }
    let end = opened.end;
    let [
        header,
        outcome,
        inputs,
        counters,
        edits,
        digests,
        verdict,
        binding,
        spellings,
        names,
        negatives,
        rule_use,
    ] = opened.sections;

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

    let (edits, edits_at) = match edits {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = edits::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };
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
    let (binding, binding_at) = match binding {
        None => (None, 0),
        Some(mut d) => {
            let (v, at) = agreement::decode_binding(&mut d)?;
            d.finish()?;
            (Some(v), at)
        }
    };
    let (spellings, spellings_at) = match spellings {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = spellings::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };
    let (names, names_at) = match names {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = names::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };
    let (negatives, negatives_at) = match negatives {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = negatives::decode(&mut d)?;
            d.finish()?;
            (Some(v), Some(at))
        }
    };
    let (rule_use, rule_use_at) = match rule_use {
        None => (None, None),
        Some(mut d) => {
            let (v, at) = agreement::decode_rule_use(&mut d)?;
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
        edits,
        binding,
        rule_use,
        spellings,
        names,
        negatives,
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
            Loc::Edits(l) => edits_at
                .as_ref()
                .map_or_else(|| unreachable!("an edits violation without edits"), |o| o.of(l)),
            Loc::Agreement(agreement::Loc::Binding) => binding_at,
            Loc::Agreement(l) => rule_use_at
                .as_ref()
                .map_or_else(|| unreachable!("a rule_use violation without rule_use"), |o| o.of(l)),
            Loc::Spellings(l) => spellings_at.as_ref().map_or_else(
                || unreachable!("a spellings violation without spellings"),
                |o| o.of(l),
            ),
            Loc::Names(l) => names_at
                .as_ref()
                .map_or_else(|| unreachable!("a names violation without names"), |o| o.of(l)),
            Loc::NoNames => digests_at.as_ref().map_or_else(
                || unreachable!("no names section without digests"),
                |o| o.of(digests::Loc::Section),
            ),
            Loc::Negatives(l) => negatives_at.as_ref().map_or_else(
                || unreachable!("a negatives violation without negatives"),
                |o| o.of(l),
            ),
        };
        return Err(malformed(at, what));
    }
    Ok((shard, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const SECTION_NAMES: [SectionName; 12] = [
    SectionName { id: HEADER, name: "header", required: true },
    SectionName { id: OUTCOME, name: "outcome", required: true },
    SectionName { id: INPUTS, name: "inputs", required: false },
    SectionName { id: COUNTERS, name: "counters", required: true },
    SectionName { id: EDITS, name: "edits", required: false },
    SectionName { id: DIGESTS, name: "digests", required: false },
    SectionName { id: VERDICT, name: "verdict", required: false },
    SectionName { id: BINDING, name: "binding", required: false },
    SectionName { id: SPELLINGS, name: "spellings", required: false },
    SectionName { id: NAMES, name: "names", required: false },
    SectionName { id: NEGATIVES, name: "negatives", required: false },
    SectionName { id: RULE_USE, name: "rule_use", required: false },
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
    if let Some(x) = &s.edits {
        sections.push(("edits", edits::render(x)));
    }
    if let Some(d) = &s.digests {
        sections.push(("digests", digests::render(d)));
    }
    if let Some(v) = &s.verdict {
        sections.push(("verdict", verdict::render(v)));
    }
    if let Some(b) = &s.binding {
        sections.push(("binding", agreement::render_binding(b)));
    }
    if let Some(x) = &s.spellings {
        sections.push(("spellings", spellings::render(x)));
    }
    if let Some(x) = &s.names {
        sections.push(("names", names::render(x)));
    }
    if let Some(x) = &s.negatives {
        sections.push(("negatives", negatives::render(x)));
    }
    if let Some(r) = &s.rule_use {
        sections.push(("rule_use", agreement::render_rule_use(r)));
    }
    render_container(Shard::KIND, Shard::VERSION, sections, extras)
}

/// The shard and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(Shard, OwnedExtras)> {
    let s = read_container(v, Shard::KIND, Shard::VERSION, &SECTION_NAMES)?;
    if s.version < FIRST_READ {
        return Err(At::Root.key("version").error(OLDER));
    }
    let [
        Some(header),
        Some(outcome),
        inputs,
        Some(counters),
        edits,
        digests,
        verdict,
        binding,
        spellings,
        names,
        negatives,
        rule_use,
    ] = s.known
    else {
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

    let edits = edits.map(|v| edits::read(v, &sections.key("edits"))).transpose()?;
    let digests = digests.map(|v| digests::read(v, &sections.key("digests"))).transpose()?;
    let verdict = verdict.map(|v| verdict::read(v, &sections.key("verdict"))).transpose()?;
    let binding =
        binding.map(|v| agreement::read_binding(v, &sections.key("binding"))).transpose()?;
    let rule_use =
        rule_use.map(|v| agreement::read_rule_use(v, &sections.key("rule_use"))).transpose()?;
    let spellings =
        spellings.map(|v| spellings::read(v, &sections.key("spellings"))).transpose()?;
    let names = names.map(|v| names::read(v, &sections.key("names"))).transpose()?;
    let negatives =
        negatives.map(|v| negatives::read(v, &sections.key("negatives"))).transpose()?;

    let shard = Shard {
        observer,
        phase,
        round,
        outcome,
        inputs,
        counters,
        digests,
        verdict,
        edits,
        binding,
        rule_use,
        spellings,
        names,
        negatives,
    };
    if let Some((l, what)) = violation(&shard) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((shard, s.extras))
}
