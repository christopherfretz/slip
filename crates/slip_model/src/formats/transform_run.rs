//! The transform run record, kind `RUNT` (`spec/edits.md` §10.6). Work
//! package WP-061. Not deterministic; no invariants beyond its encodings.

use crate::Digest;
use crate::json::Value;
use crate::transform_run::{Step, StepName, TransformRun};
use crate::wire::{Decoder, Encoder};

use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_digest, json_enum,
    json_opt, json_uv, read_digest, read_enum, read_list, read_opt, read_record, read_str, read_uv,
};
use super::edits::{enc_opt, record};
use super::{At, Extra, OwnedExtras, SectionName, open, read_container, render_container, seal};

/// The section id (§10.6).
const RUN: u64 = 1;

impl Enumeration for StepName {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (StepName::Plan, 0, "plan"),
        (StepName::Map, 1, "map"),
        (StepName::Reduce, 2, "reduce"),
        (StepName::Apply, 3, "apply"),
        (StepName::Verify, 4, "verify"),
        (StepName::Scan, 5, "scan"),
        (StepName::Conclude, 6, "conclude"),
        (StepName::Write, 7, "write"),
        (StepName::Load, 8, "load"),
    ];
    const UNLISTED: &'static str = "unlisted StepName value";
    const EXPECTED: &'static str = "expected one of \"plan\", \"map\", \"reduce\", \"apply\", \
         \"verify\", \"scan\", \"conclude\", \"write\", \"load\"";
}

impl StepName {
    /// Every value, in code order.
    pub const ALL: [StepName; 9] = [
        StepName::Plan,
        StepName::Map,
        StepName::Reduce,
        StepName::Apply,
        StepName::Verify,
        StepName::Scan,
        StepName::Conclude,
        StepName::Write,
        StepName::Load,
    ];

    /// The name of `spec/edits.md` §10.6 (`plan`, …), as in JSON.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

/// The record has no invariants beyond its encodings (§10.6).
#[allow(clippy::unnecessary_wraps)]
pub(super) fn check(_: &TransformRun) -> std::result::Result<(), (String, &'static str)> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Binary

fn enc_opt_digest(e: &mut Encoder, d: Option<&Digest>) {
    enc_opt(e, d, |e, d| {
        e.digest(d);
    });
}

pub(super) fn encode(x: &TransformRun, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.str(&x.tool_slip).str(&x.tool_kiln).digest(&x.plan).uv(x.round).digest(&x.transform);
    for d in [&x.edits, &x.declared, &x.overlay, &x.result] {
        enc_opt_digest(&mut e, d.as_ref());
    }
    e.uv(x.started_unix_ns).uv(x.wall_ns).count(x.steps.len());
    for s in &x.steps {
        enc_enum(&mut e, s.name);
        e.uv(s.wall_ns).uv(s.cpu_ns).uv(s.peak_rss);
        enc_opt_digest(&mut e, s.run.as_ref());
    }
    seal(TransformRun::KIND, TransformRun::VERSION, &[(RUN, e.as_bytes())], extras)
}

fn dec_opt_digest(d: &mut Decoder<'_>) -> Result<Option<Digest>> {
    dec_opt(d, Decoder::digest)
}

/// The record, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(TransformRun, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, TransformRun::KIND, TransformRun::VERSION, [RUN])?;
    let [section] = opened.sections;
    let mut d = super::required(section, opened.end, "transform run record has no run section")?;
    let x = TransformRun {
        tool_slip: d.str()?.to_owned(),
        tool_kiln: d.str()?.to_owned(),
        plan: d.digest()?,
        round: d.uv()?,
        transform: d.digest()?,
        edits: dec_opt_digest(&mut d)?,
        declared: dec_opt_digest(&mut d)?,
        overlay: dec_opt_digest(&mut d)?,
        result: dec_opt_digest(&mut d)?,
        started_unix_ns: d.uv()?,
        wall_ns: d.uv()?,
        steps: dec_list(&mut d, None, |d| {
            Ok(Step {
                name: dec_enum(d)?,
                wall_ns: d.uv()?,
                cpu_ns: d.uv()?,
                peak_rss: d.uv()?,
                run: dec_opt_digest(d)?,
            })
        })?,
    };
    d.finish()?;
    Ok((x, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: RUN, name: "run", required: true }];

const FIELDS: [&str; 12] = [
    "tool_slip",
    "tool_kiln",
    "plan",
    "round",
    "transform",
    "edits",
    "declared",
    "overlay",
    "result",
    "started_unix_ns",
    "wall_ns",
    "steps",
];
const STEP: [&str; 5] = ["name", "wall_ns", "cpu_ns", "peak_rss", "run"];

pub(super) fn render(x: &TransformRun, extras: &[Extra<'_>]) -> Value {
    let steps = x
        .steps
        .iter()
        .map(|s| {
            record(
                STEP,
                [
                    json_enum(s.name),
                    json_uv(s.wall_ns),
                    json_uv(s.cpu_ns),
                    json_uv(s.peak_rss),
                    json_opt(s.run.as_ref(), json_digest),
                ],
            )
        })
        .collect();
    let run = record(
        FIELDS,
        [
            Value::Str(x.tool_slip.clone()),
            Value::Str(x.tool_kiln.clone()),
            json_digest(&x.plan),
            json_uv(x.round),
            json_digest(&x.transform),
            json_opt(x.edits.as_ref(), json_digest),
            json_opt(x.declared.as_ref(), json_digest),
            json_opt(x.overlay.as_ref(), json_digest),
            json_opt(x.result.as_ref(), json_digest),
            json_uv(x.started_unix_ns),
            json_uv(x.wall_ns),
            Value::Array(steps),
        ],
    );
    render_container(TransformRun::KIND, TransformRun::VERSION, vec![("run", run)], extras)
}

/// The record and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(TransformRun, OwnedExtras)> {
    let s = read_container(v, TransformRun::KIND, TransformRun::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("run");
    let [
        tool_slip,
        tool_kiln,
        plan,
        round,
        transform,
        edits,
        declared,
        overlay,
        result,
        started,
        wall,
        steps,
    ] = read_record(section, &at, FIELDS)?;
    let x = TransformRun {
        tool_slip: read_str(tool_slip, &at.key("tool_slip"))?.to_owned(),
        tool_kiln: read_str(tool_kiln, &at.key("tool_kiln"))?.to_owned(),
        plan: read_digest(plan, &at.key("plan"))?,
        round: read_uv(round, &at.key("round"))?,
        transform: read_digest(transform, &at.key("transform"))?,
        edits: read_opt(edits, &at.key("edits"), read_digest)?,
        declared: read_opt(declared, &at.key("declared"), read_digest)?,
        overlay: read_opt(overlay, &at.key("overlay"), read_digest)?,
        result: read_opt(result, &at.key("result"), read_digest)?,
        started_unix_ns: read_uv(started, &at.key("started_unix_ns"))?,
        wall_ns: read_uv(wall, &at.key("wall_ns"))?,
        steps: read_list(steps, &at.key("steps"), |v, at| {
            let [name, wall, cpu, peak, run] = read_record(v, at, STEP)?;
            Ok(Step {
                name: read_enum(name, &at.key("name"))?,
                wall_ns: read_uv(wall, &at.key("wall_ns"))?,
                cpu_ns: read_uv(cpu, &at.key("cpu_ns"))?,
                peak_rss: read_uv(peak, &at.key("peak_rss"))?,
                run: read_opt(run, &at.key("run"), read_digest)?,
            })
        })?,
    };
    Ok((x, s.extras))
}

/// `(plan, round, started_unix_ns)` of a transform run record, once the
/// whole record has been validated as by `TransformRun::from_bytes`.
pub(super) fn names(bytes: &[u8]) -> Result<(Digest, u64, u64)> {
    let (x, _, _) = decode(bytes)?;
    Ok((x.plan, x.round, x.started_unix_ns))
}
