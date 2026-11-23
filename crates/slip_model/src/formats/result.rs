//! The result, kind `RSLT` (`spec/edits.md` §8.5), with its residue
//! (§8.4). Work package WP-061; the order of checks, the messages and the
//! JSON form are `spec/landed/edits/WP-061.md` §4.

use crate::Digest;
use crate::json::Value;
use crate::result::{
    Category, FailedSpelling, Residue, RunVerdict, SCHEME, Spellings, Stale, State, Tally,
    TransformResult, Why,
};
use crate::transform::is_file_path;
use crate::wire::{Decoder, Encoder};

use super::common::{
    Enumeration, JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_bytes, json_digest,
    json_enum, json_opt, json_uv, malformed, read_bool, read_bytes, read_digest, read_enum,
    read_list, read_opt, read_record, read_str, read_uv,
};
use super::edits::{ascending, enc_opt, record};
use super::{
    At, Extra, JsonFormError, OwnedExtras, SectionName, open, read_container, render_container,
    seal,
};

/// The section id (§8.5).
const RESULT: u64 = 1;

impl Enumeration for Category {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (Category::Dependent, 0, "dependent"),
        (Category::NotCompiled, 1, "not_compiled"),
        (Category::NotPlanned, 2, "not_planned"),
        (Category::Mention, 3, "mention"),
    ];
    const UNLISTED: &'static str = "unlisted Category value";
    const EXPECTED: &'static str =
        "expected one of \"dependent\", \"not_compiled\", \"not_planned\", \"mention\"";
}

impl Enumeration for RunVerdict {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (RunVerdict::Verified, 0, "verified"),
        (RunVerdict::NotVerified, 1, "not_verified"),
        (RunVerdict::Incomplete, 2, "incomplete"),
    ];
    const UNLISTED: &'static str = "unlisted RunVerdict value";
    const EXPECTED: &'static str = "expected one of \"verified\", \"not_verified\", \"incomplete\"";
}

impl Enumeration for Why {
    const TABLE: &'static [(Self, u64, &'static str)] =
        &[(Why::DenotesOther, 0, "denotes_other"), (Why::Unseen, 1, "unseen")];
    const UNLISTED: &'static str = "unlisted why value";
    const EXPECTED: &'static str = "expected one of \"denotes_other\", \"unseen\"";
}

impl Enumeration for State {
    const TABLE: &'static [(Self, u64, &'static str)] = &[
        (State::Verified, 0, "verified"),
        (State::VerifiedWithResidue, 1, "verified_with_residue"),
        (State::NotVerified, 2, "not_verified"),
        (State::Incomplete, 3, "incomplete"),
        (State::Refused, 4, "refused"),
    ];
    const UNLISTED: &'static str = "unlisted State value";
    const EXPECTED: &'static str = "expected one of \"verified\", \"verified_with_residue\", \
         \"not_verified\", \"incomplete\", \"refused\"";
}

impl Category {
    /// Every value, in code order.
    pub const ALL: [Category; 4] =
        [Category::Dependent, Category::NotCompiled, Category::NotPlanned, Category::Mention];

    /// The name of `spec/edits.md` §8.4 (`dependent`, …), as in JSON.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl RunVerdict {
    /// Every value, in code order.
    pub const ALL: [RunVerdict; 3] =
        [RunVerdict::Verified, RunVerdict::NotVerified, RunVerdict::Incomplete];

    /// The name of `spec/verify.md` §7 (`verified`, …), as in JSON and in
    /// the output.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl Why {
    /// Every value, in code order.
    pub const ALL: [Why; 2] = [Why::DenotesOther, Why::Unseen];

    /// The name of `spec/edits.md` §8.5 (`denotes_other`, `unseen`), as in JSON.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

impl State {
    /// Every value, in code order.
    pub const ALL: [State; 5] = [
        State::Verified,
        State::VerifiedWithResidue,
        State::NotVerified,
        State::Incomplete,
        State::Refused,
    ];

    /// The name of `spec/edits.md` §8.5 (`verified`, …), as in JSON.
    pub fn name(self) -> &'static str {
        Enumeration::name(self)
    }
}

// ---------------------------------------------------------------------------
// Invariants (§8.5, in the order of its sentence)

#[derive(Clone, Copy, Debug)]
enum Loc {
    Scheme,
    Declared,
    Overlay,
    State,
    Verdict,
    Tally(usize),
    Agreement(usize),
    Unused(usize),
    Unchecked(usize),
    Obligations,
    Failed(usize),
    FailedObservers(usize),
    FailedFirst(usize),
    FailedFound(usize),
    Residue(usize),
    ResidueFile(usize),
    ResidueLo(usize),
    ResidueLine(usize),
    ResidueColumn(usize),
    Exempt(usize),
}

const TALLY: [&str; 11] = [
    "selected",
    "unparsed",
    "failed_runs",
    "error_reports",
    "verified",
    "unchanged",
    "mismatch",
    "not_parsed",
    "unverifiable",
    "never_parsed",
    "failed",
];

/// The index in [`TALLY`] of the first field a refused result must have 0 in.
const TALLY_ZERO: usize = 4;

fn tally_fields(t: &Tally) -> [u64; 11] {
    [
        t.selected,
        t.unparsed,
        t.failed_runs,
        t.error_reports,
        t.verified,
        t.unchanged,
        t.mismatch,
        t.not_parsed,
        t.unverifiable,
        t.never_parsed,
        t.failed,
    ]
}

impl Loc {
    fn path(self) -> String {
        const S: &str = "sections.result";
        match self {
            Loc::Scheme => format!("{S}.scheme"),
            Loc::Declared => format!("{S}.declared"),
            Loc::Overlay => format!("{S}.overlay"),
            Loc::State => format!("{S}.state"),
            Loc::Verdict => format!("{S}.verdict"),
            Loc::Tally(i) => format!("{S}.tally.{}", TALLY[i]),
            Loc::Agreement(i) => format!("{S}.agreement[{i}]"),
            Loc::Unused(i) => format!("{S}.unused[{i}]"),
            Loc::Unchecked(i) => format!("{S}.unchecked[{i}]"),
            Loc::Obligations => format!("{S}.spellings.obligations"),
            Loc::Failed(i) => format!("{S}.spellings.failed[{i}]"),
            Loc::FailedObservers(i) => format!("{S}.spellings.failed[{i}].observers"),
            Loc::FailedFirst(i) => format!("{S}.spellings.failed[{i}].first"),
            Loc::FailedFound(i) => format!("{S}.spellings.failed[{i}].found"),
            Loc::Residue(i) => format!("{S}.residue[{i}]"),
            Loc::ResidueFile(i) => format!("{S}.residue[{i}].file"),
            Loc::ResidueLo(i) => format!("{S}.residue[{i}].lo"),
            Loc::ResidueLine(i) => format!("{S}.residue[{i}].line"),
            Loc::ResidueColumn(i) => format!("{S}.residue[{i}].column"),
            Loc::Exempt(i) => format!("{S}.exempt[{i}]"),
        }
    }
}

/// The first violation: `scheme`, then the invariants of §8.5 in its order
/// (`spec/landed/edits/WP-061.md` §4, rows 6a, 8a, 13 and 14 from
/// `spec/landed/verify/WP-070.md` §1).
#[allow(clippy::too_many_lines, reason = "one statement per invariant of spec/edits.md §8.5")]
fn violation(x: &TransformResult) -> Option<(Loc, &'static str)> {
    if x.scheme != SCHEME {
        return Some((Loc::Scheme, "unknown result scheme"));
    }
    if let Some(i) = (1..x.agreement.len()).find(|&i| {
        (x.agreement[i - 1].rule, x.agreement[i - 1].observer)
            >= (x.agreement[i].rule, x.agreement[i].observer)
    }) {
        return Some((
            Loc::Agreement(i),
            "agreement is not strictly ascending by (rule, observer)",
        ));
    }
    if let Some(i) = ascending(&x.unused) {
        return Some((Loc::Unused(i), "unused is not strictly ascending"));
    }
    if let Some(i) = ascending(&x.unchecked) {
        return Some((Loc::Unchecked(i), "unchecked is not strictly ascending"));
    }
    if let Some(i) = ascending(&x.exempt) {
        return Some((Loc::Exempt(i), "exempt is not strictly ascending"));
    }
    let f = &x.spellings.failed;
    if let Some(i) = (1..f.len()).find(|&i| f[i - 1].obligation >= f[i].obligation) {
        return Some((Loc::Failed(i), "failed spellings are not strictly ascending by obligation"));
    }
    let len = u64::try_from(f.len()).ok();
    if len.and_then(|n| x.spellings.passed.checked_add(n)) != Some(x.spellings.obligations) {
        return Some((
            Loc::Obligations,
            "passed and the failed spellings do not add up to obligations",
        ));
    }
    // Row 6a: what `why` alone decides of a failed spelling (§8.5): an
    // `unseen` obligation was checked by no observer, a `denotes_other`
    // one failed in at least one, the first of which is named.
    for (i, s) in f.iter().enumerate() {
        let unseen = s.why == Why::Unseen;
        if (s.observers == 0) != unseen {
            return Some((
                Loc::FailedObservers(i),
                "observers is not 0 for unseen or is 0 for denotes_other",
            ));
        }
        if s.first.is_none() != unseen {
            return Some((
                Loc::FailedFirst(i),
                "first is present for unseen or absent for denotes_other",
            ));
        }
        if unseen && s.found.is_some() {
            return Some((Loc::FailedFound(i), "found is present for unseen"));
        }
    }
    let r = &x.residue;
    if let Some(i) = r.iter().position(|e| !is_file_path(&e.file)) {
        return Some((Loc::ResidueFile(i), super::editset::path_what(&r[i].file)));
    }
    if let Some(i) = (1..r.len()).find(|&i| (&r[i - 1].file, r[i - 1].lo) >= (&r[i].file, r[i].lo))
    {
        return Some((Loc::Residue(i), "residue is not strictly ascending by (file, lo)"));
    }
    // Row 8a: a residue entry is a token or a mention, never empty; line
    // and column are 1-based (§8.4).
    for (i, e) in r.iter().enumerate() {
        if e.lo >= e.hi {
            return Some((Loc::ResidueLo(i), "a residue's range is empty"));
        }
        if e.line == 0 {
            return Some((Loc::ResidueLine(i), "a residue's line is 0: lines are 1-based"));
        }
        if e.column == 0 {
            return Some((Loc::ResidueColumn(i), "a residue's column is 0: columns are 1-based"));
        }
    }
    let refused = x.state == State::Refused;
    if x.overlay.is_none() != refused {
        return Some((
            Loc::Overlay,
            "overlay is absent for a state other than refused or present for refused",
        ));
    }
    if x.declared.is_none() != refused {
        return Some((
            Loc::Declared,
            "declared is absent for a state other than refused or present for refused",
        ));
    }
    if refused {
        if x.verdict.is_some() {
            return Some((Loc::Verdict, "a refused result has a verdict"));
        }
        if let Some(i) = (TALLY_ZERO..TALLY.len()).find(|&i| tally_fields(&x.tally)[i] != 0) {
            return Some((Loc::Tally(i), "a refused result has a count of observers verified"));
        }
    }
    // Rows 13 and 14: the tally adds up (§8.5). The seven states are those
    // of the observers the verify phase selected, which are the run's
    // selected observers less the unparsed ones; with no verdict there was
    // no finished verify phase and no state.
    let t = tally_fields(&x.tally);
    let Some(parsed) = x.tally.selected.checked_sub(x.tally.unparsed) else {
        return Some((Loc::Tally(1), "unparsed is more than selected"));
    };
    if x.verdict.is_some() {
        let sum = t[TALLY_ZERO..].iter().try_fold(0u64, |a, &v| a.checked_add(v));
        if sum != Some(parsed) {
            return Some((
                Loc::Tally(TALLY_ZERO),
                "the states of the tally do not add up to selected less unparsed",
            ));
        }
    } else if let Some(i) = (TALLY_ZERO..TALLY.len()).find(|&i| t[i] != 0) {
        return Some((
            Loc::Tally(i),
            "a result without a verdict has a count of observers verified",
        ));
    }
    verdict_violation(x)
}

/// Rows 15 to 17 (`spec/landed/verify/WP-070.md` §1): what the file alone
/// decides of the verdict and the state (§8.5, `verify.md` §9.2, §9.3).
fn verdict_violation(x: &TransformResult) -> Option<(Loc, &'static str)> {
    // Row 15: a stale pair is one with `direct` not 0 (verify.md §9.2).
    if let Some(i) = x.agreement.iter().position(|s| s.direct == 0) {
        return Some((Loc::Agreement(i), "a stale pair has direct 0"));
    }
    // Row 16: the verdict `verified` has no stale pair and no failed
    // obligation (verify.md §9.3); without a verdict no rule is unchecked.
    match x.verdict {
        Some(RunVerdict::Verified) => {
            if !x.agreement.is_empty() {
                return Some((Loc::Agreement(0), "a verified verdict has a stale pair"));
            }
            if !x.spellings.failed.is_empty() {
                return Some((Loc::Failed(0), "a verified verdict has a failed spelling"));
            }
        }
        None if !x.unchecked.is_empty() => {
            return Some((Loc::Unchecked(0), "a result without a verdict has an unchecked rule"));
        }
        _ => {}
    }
    // Row 17: the state against the verdict (rows 2 to 5 of §8.5).
    let unparsed_bars = x.tally.unparsed != 0 && !x.allow_unparsed;
    let barred = unparsed_bars || !x.unchecked.is_empty();
    let verified = x.verdict == Some(RunVerdict::Verified);
    let contradicts = match x.state {
        State::Verified | State::VerifiedWithResidue => !verified || barred,
        State::NotVerified => x.verdict != Some(RunVerdict::NotVerified),
        State::Incomplete => x.verdict == Some(RunVerdict::NotVerified) || (verified && !barred),
        State::Refused => false,
    };
    if contradicts {
        return Some((Loc::State, "the state is not the one the verdict and the tally give"));
    }
    let residue = x.residue.iter().any(|r| r.category != Category::Mention);
    if x.state == State::Verified
        && (residue || x.tally.failed_runs != 0 || x.tally.error_reports != 0)
    {
        return Some((Loc::State, "a verified result has residue, a failed run or an error"));
    }
    None
}

pub(super) fn check(x: &TransformResult) -> std::result::Result<(), (String, &'static str)> {
    violation(x).map_or(Ok(()), |(l, what)| Err((l.path(), what)))
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(x: &TransformResult, extras: &[Extra<'_>]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.uv(x.scheme).digest(&x.plan).uv(x.round).digest(&x.transform).digest(&x.edits);
    enc_opt(&mut e, x.declared.as_ref(), |e, d| {
        e.digest(d);
    });
    enc_opt(&mut e, x.overlay.as_ref(), |e, d| {
        e.digest(d);
    });
    enc_enum(&mut e, x.state);
    e.bool(x.partial).bool(x.allow_unparsed);
    enc_opt(&mut e, x.verdict.as_ref(), |e, v| enc_enum(e, *v));
    for v in tally_fields(&x.tally) {
        e.uv(v);
    }
    e.count(x.agreement.len());
    for s in &x.agreement {
        e.uv(s.rule).digest(&s.observer).uv(s.direct);
    }
    e.count(x.unused.len());
    for &u in &x.unused {
        e.uv(u);
    }
    e.count(x.unchecked.len());
    for &u in &x.unchecked {
        e.uv(u);
    }
    let s = &x.spellings;
    e.uv(s.obligations).uv(s.passed).count(s.failed.len());
    for f in &s.failed {
        e.uv(f.obligation);
        enc_enum(&mut e, f.why);
        e.uv(f.observers);
        enc_opt(&mut e, f.first.as_ref(), |e, d| {
            e.digest(d);
        });
        enc_opt(&mut e, f.found.as_ref(), |e, d| {
            e.digest(d);
        });
        e.str(&f.name);
    }
    e.count(x.residue.len());
    for r in &x.residue {
        enc_enum(&mut e, r.category);
        e.bytes(&r.file).uv(r.lo).uv(r.hi).uv(r.line).uv(r.column);
    }
    e.count(x.exempt.len());
    for k in &x.exempt {
        e.digest(k);
    }
    seal(TransformResult::KIND, TransformResult::VERSION, &[(RESULT, e.as_bytes())], extras)
}

#[derive(Default)]
struct Offsets {
    scheme: usize,
    declared: usize,
    overlay: usize,
    state: usize,
    verdict: usize,
    tally: Vec<usize>,
    agreement: Vec<usize>,
    unused: Vec<usize>,
    unchecked: Vec<usize>,
    obligations: usize,
    failed: Vec<usize>,
    /// Per failed spelling: `observers`, `first`, `found`.
    failed_fields: Vec<[usize; 3]>,
    /// Per residue entry: its start, `file`, `lo`, `line`, `column`.
    residue: Vec<[usize; 5]>,
    exempt: Vec<usize>,
}

impl Offsets {
    fn of(&self, l: Loc) -> usize {
        match l {
            Loc::Scheme => self.scheme,
            Loc::Declared => self.declared,
            Loc::Overlay => self.overlay,
            Loc::State => self.state,
            Loc::Verdict => self.verdict,
            Loc::Tally(i) => self.tally[i],
            Loc::Agreement(i) => self.agreement[i],
            Loc::Unused(i) => self.unused[i],
            Loc::Unchecked(i) => self.unchecked[i],
            Loc::Obligations => self.obligations,
            Loc::Failed(i) => self.failed[i],
            Loc::FailedObservers(i) => self.failed_fields[i][0],
            Loc::FailedFirst(i) => self.failed_fields[i][1],
            Loc::FailedFound(i) => self.failed_fields[i][2],
            Loc::Residue(i) => self.residue[i][0],
            Loc::ResidueFile(i) => self.residue[i][1],
            Loc::ResidueLo(i) => self.residue[i][2],
            Loc::ResidueLine(i) => self.residue[i][3],
            Loc::ResidueColumn(i) => self.residue[i][4],
            Loc::Exempt(i) => self.exempt[i],
        }
    }
}

fn dec_opt_digest(d: &mut Decoder<'_>) -> Result<Option<Digest>> {
    dec_opt(d, Decoder::digest)
}

/// The result, its unknown ignorable sections and its content id.
#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §8.5")]
pub(super) fn decode(bytes: &[u8]) -> Result<(TransformResult, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, TransformResult::KIND, TransformResult::VERSION, [RESULT])?;
    let [section] = opened.sections;
    let mut section = super::required(section, opened.end, "result has no result section")?;
    let d = &mut section;
    let mut o = Offsets { scheme: d.offset(), ..Offsets::default() };
    let scheme = d.uv()?;
    let plan = d.digest()?;
    let round = d.uv()?;
    let transform = d.digest()?;
    let edits = d.digest()?;
    o.declared = d.offset();
    let declared = dec_opt_digest(d)?;
    o.overlay = d.offset();
    let overlay = dec_opt_digest(d)?;
    o.state = d.offset();
    let state = dec_enum(d)?;
    let partial = d.bool()?;
    let allow_unparsed = d.bool()?;
    o.verdict = d.offset();
    let verdict = dec_opt(d, dec_enum)?;
    let mut t = [0u64; 11];
    for v in &mut t {
        o.tally.push(d.offset());
        *v = d.uv()?;
    }
    let [
        selected,
        unparsed,
        failed_runs,
        error_reports,
        verified,
        unchanged,
        mismatch,
        not_parsed,
        unverifiable,
        never_parsed,
        failed,
    ] = t;
    let tally = Tally {
        selected,
        unparsed,
        failed_runs,
        error_reports,
        verified,
        unchanged,
        mismatch,
        not_parsed,
        unverifiable,
        never_parsed,
        failed,
    };
    let agreement = dec_list(d, Some(&mut o.agreement), |d| {
        Ok(Stale { rule: d.uv()?, observer: d.digest()?, direct: d.uv()? })
    })?;
    let unused = dec_list(d, Some(&mut o.unused), Decoder::uv)?;
    let unchecked = dec_list(d, Some(&mut o.unchecked), Decoder::uv)?;
    o.obligations = d.offset();
    let obligations = d.uv()?;
    let passed = d.uv()?;
    let mut failed_fields = Vec::new();
    let failed = dec_list(d, Some(&mut o.failed), |d| {
        let obligation = d.uv()?;
        let why = dec_enum(d)?;
        let at_observers = d.offset();
        let observers = d.uv()?;
        let at_first = d.offset();
        let first = dec_opt_digest(d)?;
        let at_found = d.offset();
        let found = dec_opt_digest(d)?;
        failed_fields.push([at_observers, at_first, at_found]);
        Ok(FailedSpelling { obligation, why, observers, first, found, name: d.str()?.to_owned() })
    })?;
    o.failed_fields = failed_fields;
    let residue = dec_list(d, None, |d| {
        let start = d.offset();
        let category = dec_enum(d)?;
        let file_at = d.offset();
        let file = d.bytes()?.to_vec();
        let lo_at = d.offset();
        let lo = d.uv()?;
        let hi = d.uv()?;
        let line_at = d.offset();
        let line = d.uv()?;
        let column_at = d.offset();
        let column = d.uv()?;
        o.residue.push([start, file_at, lo_at, line_at, column_at]);
        Ok(Residue { category, file, lo, hi, line, column })
    })?;
    let exempt = dec_list(d, Some(&mut o.exempt), Decoder::digest)?;
    section.finish()?;
    let x = TransformResult {
        scheme,
        plan,
        round,
        transform,
        edits,
        declared,
        overlay,
        state,
        partial,
        allow_unparsed,
        verdict,
        tally,
        agreement,
        unused,
        unchecked,
        spellings: Spellings { obligations, passed, failed },
        residue,
        exempt,
    };
    if let Some((l, what)) = violation(&x) {
        return Err(malformed(o.of(l), what));
    }
    Ok((x, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 1] = [SectionName { id: RESULT, name: "result", required: true }];

const FIELDS: [&str; 18] = [
    "scheme",
    "plan",
    "round",
    "transform",
    "edits",
    "declared",
    "overlay",
    "state",
    "partial",
    "allow_unparsed",
    "verdict",
    "tally",
    "agreement",
    "unused",
    "unchecked",
    "spellings",
    "residue",
    "exempt",
];
const STALE: [&str; 3] = ["rule", "observer", "direct"];
const SPELLINGS: [&str; 3] = ["obligations", "passed", "failed"];
const FAILED: [&str; 6] = ["obligation", "why", "observers", "first", "found", "name"];
const RESIDUE: [&str; 6] = ["category", "file", "lo", "hi", "line", "column"];

pub(super) fn render(x: &TransformResult, extras: &[Extra<'_>]) -> Value {
    let s = &x.spellings;
    let failed = s
        .failed
        .iter()
        .map(|f| {
            record(
                FAILED,
                [
                    json_uv(f.obligation),
                    json_enum(f.why),
                    json_uv(f.observers),
                    json_opt(f.first.as_ref(), json_digest),
                    json_opt(f.found.as_ref(), json_digest),
                    Value::Str(f.name.clone()),
                ],
            )
        })
        .collect();
    let result = record(
        FIELDS,
        [
            json_uv(x.scheme),
            json_digest(&x.plan),
            json_uv(x.round),
            json_digest(&x.transform),
            json_digest(&x.edits),
            json_opt(x.declared.as_ref(), json_digest),
            json_opt(x.overlay.as_ref(), json_digest),
            json_enum(x.state),
            Value::Bool(x.partial),
            Value::Bool(x.allow_unparsed),
            json_opt(x.verdict.as_ref(), |&v| json_enum(v)),
            record(TALLY, tally_fields(&x.tally).map(json_uv)),
            Value::Array(
                x.agreement
                    .iter()
                    .map(|a| {
                        record(
                            STALE,
                            [json_uv(a.rule), json_digest(&a.observer), json_uv(a.direct)],
                        )
                    })
                    .collect(),
            ),
            Value::Array(x.unused.iter().map(|&u| json_uv(u)).collect()),
            Value::Array(x.unchecked.iter().map(|&u| json_uv(u)).collect()),
            record(SPELLINGS, [json_uv(s.obligations), json_uv(s.passed), Value::Array(failed)]),
            Value::Array(
                x.residue
                    .iter()
                    .map(|r| {
                        record(
                            RESIDUE,
                            [
                                json_enum(r.category),
                                json_bytes(&r.file),
                                json_uv(r.lo),
                                json_uv(r.hi),
                                json_uv(r.line),
                                json_uv(r.column),
                            ],
                        )
                    })
                    .collect(),
            ),
            Value::Array(x.exempt.iter().map(json_digest).collect()),
        ],
    );
    render_container(
        TransformResult::KIND,
        TransformResult::VERSION,
        vec![("result", result)],
        extras,
    )
}

/// The result and the unknown ignorable sections of a JSON rendering.
#[allow(clippy::too_many_lines, reason = "one statement per field of spec/edits.md §8.5")]
pub(super) fn read(v: &Value) -> JResult<(TransformResult, OwnedExtras)> {
    let s = read_container(v, TransformResult::KIND, TransformResult::VERSION, &NAMES)?;
    let [Some(section)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");
    let at = sections.key("result");
    let [
        scheme,
        plan,
        round,
        transform,
        edits,
        declared,
        overlay,
        state,
        partial,
        allow_unparsed,
        verdict,
        tally,
        agreement,
        unused,
        unchecked,
        spellings,
        residue,
        exempt,
    ] = read_record(section, &at, FIELDS)?;
    let tally_at = at.key("tally");
    let members = read_record(tally, &tally_at, TALLY)?;
    let mut t = [0u64; 11];
    for (i, v) in members.into_iter().enumerate() {
        t[i] = read_uv(v, &tally_at.key(TALLY[i]))?;
    }
    let [
        selected,
        unparsed,
        failed_runs,
        error_reports,
        verified,
        unchanged,
        mismatch,
        not_parsed,
        unverifiable,
        never_parsed,
        failed,
    ] = t;
    let sp_at = at.key("spellings");
    let [obligations, passed, failed_list] = read_record(spellings, &sp_at, SPELLINGS)?;
    let x = TransformResult {
        scheme: read_uv(scheme, &at.key("scheme"))?,
        plan: read_digest(plan, &at.key("plan"))?,
        round: read_uv(round, &at.key("round"))?,
        transform: read_digest(transform, &at.key("transform"))?,
        edits: read_digest(edits, &at.key("edits"))?,
        declared: read_opt(declared, &at.key("declared"), read_digest)?,
        overlay: read_opt(overlay, &at.key("overlay"), read_digest)?,
        state: read_enum(state, &at.key("state"))?,
        partial: read_bool(partial, &at.key("partial"))?,
        allow_unparsed: read_bool(allow_unparsed, &at.key("allow_unparsed"))?,
        verdict: read_opt(verdict, &at.key("verdict"), read_enum)?,
        tally: Tally {
            selected,
            unparsed,
            failed_runs,
            error_reports,
            verified,
            unchanged,
            mismatch,
            not_parsed,
            unverifiable,
            never_parsed,
            failed,
        },
        agreement: read_list(agreement, &at.key("agreement"), |v, at| {
            let [rule, observer, direct] = read_record(v, at, STALE)?;
            Ok(Stale {
                rule: read_uv(rule, &at.key("rule"))?,
                observer: read_digest(observer, &at.key("observer"))?,
                direct: read_uv(direct, &at.key("direct"))?,
            })
        })?,
        unused: read_list(unused, &at.key("unused"), read_uv)?,
        unchecked: read_list(unchecked, &at.key("unchecked"), read_uv)?,
        spellings: Spellings {
            obligations: read_uv(obligations, &sp_at.key("obligations"))?,
            passed: read_uv(passed, &sp_at.key("passed"))?,
            failed: read_list(failed_list, &sp_at.key("failed"), |v, at| {
                let [obligation, why, observers, first, found, name] = read_record(v, at, FAILED)?;
                Ok(FailedSpelling {
                    obligation: read_uv(obligation, &at.key("obligation"))?,
                    why: read_enum(why, &at.key("why"))?,
                    observers: read_uv(observers, &at.key("observers"))?,
                    first: read_opt(first, &at.key("first"), read_digest)?,
                    found: read_opt(found, &at.key("found"), read_digest)?,
                    name: read_str(name, &at.key("name"))?.to_owned(),
                })
            })?,
        },
        residue: read_list(residue, &at.key("residue"), |v, at| {
            let [category, file, lo, hi, line, column] = read_record(v, at, RESIDUE)?;
            Ok(Residue {
                category: read_enum(category, &at.key("category"))?,
                file: read_bytes(file, &at.key("file"))?,
                lo: read_uv(lo, &at.key("lo"))?,
                hi: read_uv(hi, &at.key("hi"))?,
                line: read_uv(line, &at.key("line"))?,
                column: read_uv(column, &at.key("column"))?,
            })
        })?,
        exempt: read_list(exempt, &at.key("exempt"), read_digest)?,
    };
    if let Some((l, what)) = violation(&x) {
        return Err(JsonFormError { path: l.path(), message: what.to_owned() });
    }
    Ok((x, s.extras))
}
