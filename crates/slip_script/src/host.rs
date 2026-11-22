//! The host (`spec/api.md` §3): one operation at a time, as a pure
//! function over a [`Subject`].
//!
//! A call is checked in this order: the operation number (status 254),
//! the shape of its arguments (255), the operation's own refusals (an
//! `errc` of §4), then what the subject answered (a host failure if it is
//! inconsistent). Only once all of that passed does the host change: it
//! issues handles for the values it returns and writes the answer. A
//! failure changes nothing but the failure list (§3.5 rule 2).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use slip_model::{Counter, Digest};

use crate::output::RunOutput;
use crate::subject::{Entity, File, Range, Subject, Value};
use crate::wire::{self, Errc, Operation};

/// Where a host call writes its answer (Kiln's `HostOut`, without its
/// type). The same four methods as `slip_kiln::session::Out` (WP-080);
/// the driver adapts one to the other.
pub trait Out {
    fn int_capacity(&self) -> usize;
    fn byte_capacity(&self) -> usize;
    fn write_ints(&mut self, v: &[u64]);
    fn write_bytes(&mut self, v: &[u8]);
}

/// The limits of a run (§7.7). `Default` is the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub code_values: u64,
    /// Bytes in literal pieces, in total.
    pub literal_bytes: u64,
    pub pieces_per_code: u64,
    pub edits: u64,
    pub groups: u64,
    pub reports: u64,
    /// Bytes in one note, reason or message.
    pub text_bytes: u64,
    /// Bytes in one qualified name or identifier argument.
    pub name_bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            code_values: 1 << 22,
            literal_bytes: 1 << 28,
            pieces_per_code: 1 << 16,
            edits: 1 << 20,
            groups: 1 << 20,
            reports: 1 << 16,
            text_bytes: 1 << 12,
            name_bytes: 1 << 12,
        }
    }
}

/// One refused call (§3.5): its ordinal in the run (from 1), the
/// operation number as given, the status (an `errc`, 254 or 255), a
/// message for a person, and the place in the subject the offending value
/// denotes, when it denotes one (§2.8 rule 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub ordinal: u64,
    pub op: u32,
    pub errc: u8,
    pub message: String,
    pub place: Option<Range>,
}

/// Why an operation stopped before changing anything.
#[derive(Debug)]
pub(crate) enum Stop {
    /// A refusal: a status for the transform.
    Refused { errc: Errc, message: String, place: Option<Range> },
    /// A host failure (§2.8): the subject answered something inconsistent.
    Internal(String),
}

pub(crate) type Step<T> = Result<T, Stop>;

fn refuse<T>(errc: Errc, message: impl Into<String>) -> Step<T> {
    Err(Stop::Refused { errc, message: message.into(), place: None })
}

/// An answer computed and not yet delivered.
#[derive(Debug)]
pub(crate) enum Answer {
    /// Convention V: a number.
    Word(u64),
    /// Convention V: a handle, or 0.
    Info(Option<Value>),
    /// Convention B: handles.
    Values(Vec<Value>),
    /// Convention B: two integers, or none.
    Range(Option<Range>),
    /// Convention B: bytes.
    Bytes(Vec<u8>),
}

/// The host of one run.
pub struct Host<'s> {
    subject: &'s dyn Subject,
    limits: Limits,
    /// Per sort (tag - 1): the subject's number of each index issued.
    issued: [Vec<u32>; 4],
    /// Per sort: the index issued for a subject number.
    index_of: [HashMap<u32, u64>; 4],
    /// The file of each file handle issued (handle = position + 1).
    files: Vec<File>,
    file_handle: HashMap<File, u32>,
    failures: Vec<Failure>,
    /// Successful calls per operation name.
    used: BTreeMap<&'static str, u64>,
    /// What the effects recorded, raw (WP-088).
    pub(crate) record: RunOutput,
}

impl fmt::Debug for Host<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Host")
            .field("limits", &self.limits)
            .field("issued", &self.issued.iter().map(Vec::len).collect::<Vec<_>>())
            .field("files", &self.files.len())
            .field("failures", &self.failures.len())
            .finish_non_exhaustive()
    }
}

impl<'s> Host<'s> {
    pub fn new(subject: &'s dyn Subject, transform: Digest, limits: Limits) -> Host<'s> {
        Host {
            subject,
            limits,
            issued: Default::default(),
            index_of: Default::default(),
            files: Vec::new(),
            file_handle: HashMap::new(),
            failures: Vec::new(),
            used: BTreeMap::new(),
            record: RunOutput::empty(transform),
        }
    }

    /// One operation. `Err` only for a host-internal failure.
    pub fn call(
        &mut self,
        op: u32,
        ints: &[u64],
        bytes: &[u8],
        out: &mut dyn Out,
    ) -> Result<u64, String> {
        let Some(row) = wire::operation(op) else {
            return Ok(self.fail(op, wire::STATUS_UNKNOWN_OP, format!("no operation {op}"), None));
        };
        if !row.fits(ints.len(), bytes.len()) {
            let message = format!(
                "{}: takes {} and {}; given {} integers and {} bytes",
                row.name,
                match row.ints {
                    wire::Arity::Exactly(n) => format!("{n} integers"),
                    wire::Arity::Any => "any number of integers".to_owned(),
                },
                if row.text { "optional text" } else { "no text" },
                ints.len(),
                bytes.len(),
            );
            return Ok(self.fail(op, wire::STATUS_MALFORMED, message, None));
        }
        let answer = match self.answer(row, ints, bytes).and_then(|a| self.check(a)) {
            Ok(a) => a,
            Err(Stop::Refused { errc, message, place }) => {
                let message = format!("{}: {message}", row.name);
                return Ok(self.fail(op, errc.status(), message, place));
            }
            Err(Stop::Internal(m)) => return Err(format!("{}: {m}", row.name)),
        };
        let word = self.deliver(row, answer, out);
        *self.used.entry(row.name).or_default() += 1;
        Ok(word)
    }

    /// The failures of this run, in order (ordinal `i + 1` at `i`).
    pub fn failures(&self) -> &[Failure] {
        &self.failures
    }

    /// The run output in canonical form (§7.6), with the counters the host
    /// keeps: `api.edits`, `api.reports` and one `api.op.<name>` per
    /// operation that succeeded at least once. `api.host_calls`,
    /// `api.steps` and `api.peak_bytes` are the evaluator's and are added
    /// by the driver.
    pub fn finish(self) -> RunOutput {
        let mut out = self.record.canonical();
        out.counters.push(Counter { name: "api.edits".to_owned(), value: out.edits.len() as u64 });
        out.counters
            .push(Counter { name: "api.reports".to_owned(), value: out.reports.len() as u64 });
        for (name, n) in self.used {
            out.counters.push(Counter { name: format!("api.op.{name}"), value: n });
        }
        out.counters.sort();
        out.counters.dedup();
        out
    }

    // -----------------------------------------------------------------------
    // Failures.

    /// Records a failure and returns its result word.
    pub(crate) fn fail(
        &mut self,
        op: u32,
        status: u8,
        message: String,
        place: Option<Range>,
    ) -> u64 {
        let ordinal = self.failures.len() as u64 + 1;
        let place = place.filter(|r| self.range_is_valid(*r));
        self.failures.push(Failure { ordinal, op, errc: status, message, place });
        wire::failure_word(status, ordinal)
    }

    /// A refusal for a live value of the wrong sort or kind.
    pub(crate) fn wrong_kind(&self, v: Value, expected: &str) -> Stop {
        self.refusal_at(v, Errc::WrongKind, format!("expected {expected}; given {}", v.sort_name()))
    }

    /// A refusal that concerns the live value `v`, with its place.
    pub(crate) fn refusal_at(&self, v: Value, errc: Errc, message: String) -> Stop {
        Stop::Refused { errc, message, place: self.subject.place(v).map(|(r, _)| r) }
    }

    // -----------------------------------------------------------------------
    // Arguments (§3.4, §5.2).

    /// The live value of handle `h`: refused with `invalid_handle` for 0,
    /// a tag that is not a sort of `info`, or an index this host did not
    /// issue.
    pub(crate) fn value_arg(&self, h: u64) -> Step<Value> {
        if h == 0 {
            return refuse(Errc::InvalidHandle, "a null info where a value is required");
        }
        let Some(tag @ wire::TAG_ENTITY..=wire::TAG_EXPRESSION) = wire::handle_tag(h) else {
            return refuse(Errc::InvalidHandle, "not the handle of an info");
        };
        let index = wire::handle_index(h);
        let Some(&n) =
            usize::try_from(index).ok().and_then(|i| self.issued[tag as usize - 1].get(i))
        else {
            return refuse(Errc::InvalidHandle, "an info this run was not given");
        };
        match Value::from_tag(tag, n) {
            Some(v) => Ok(v),
            None => unreachable!("tag {tag} is a sort of info"),
        }
    }

    /// The live entity of handle `h`.
    pub(crate) fn entity_arg(&self, h: u64) -> Step<Entity> {
        match self.value_arg(h)? {
            Value::Entity(e) => Ok(e),
            v => Err(self.wrong_kind(v, "an entity")),
        }
    }

    /// The range of the two integers `file`, `span` (`lo << 32 | hi`):
    /// refused with `invalid_handle` when the file handle is 0 (the null
    /// range) or not one this host issued, or when not `lo <= hi <=` the
    /// file's length.
    pub(crate) fn range_arg(&self, file: u64, span: u64) -> Step<Range> {
        if file == 0 {
            return refuse(Errc::InvalidHandle, "a null range where one is required");
        }
        let Some(&f) = usize::try_from(file - 1).ok().and_then(|i| self.files.get(i)) else {
            return refuse(Errc::InvalidHandle, "a range in a file this run was not given");
        };
        let r = Range { file: f, lo: (span >> 32) as u32, hi: span as u32 };
        let Some(len) = self.subject.file_len(f) else {
            return Err(Stop::Internal("a file the subject gave out is no longer live".to_owned()));
        };
        if r.lo > r.hi || r.hi > len {
            return refuse(Errc::InvalidHandle, "a range that does not lie in its file");
        }
        Ok(r)
    }

    fn range_is_valid(&self, r: Range) -> bool {
        self.subject.file_len(r.file).is_some_and(|len| r.lo <= r.hi && r.hi <= len)
    }

    // -----------------------------------------------------------------------
    // The queries (§3.2, §6).

    #[allow(clippy::too_many_lines, reason = "one arm per operation of the table")]
    fn answer(&self, row: &Operation, ints: &[u64], bytes: &[u8]) -> Step<Answer> {
        let s = self.subject;
        Ok(match row.op {
            wire::OP_ERROR_MESSAGE => {
                let n = ints[0];
                match usize::try_from(n).ok().and_then(|n| n.checked_sub(1)) {
                    Some(i) if i < self.failures.len() => {
                        Answer::Bytes(self.failures[i].message.clone().into_bytes())
                    }
                    _ => {
                        return refuse(
                            Errc::InvalidArgument,
                            "not the ordinal of a failure of this run",
                        );
                    }
                }
            }
            wire::OP_LOOKUP => {
                if bytes.len() as u64 > self.limits.name_bytes {
                    return refuse(Errc::LimitExceeded, "a qualified name longer than the limit");
                }
                let (global, components) = match parse_qualified_name(bytes) {
                    Ok(p) => p,
                    Err(Errc::Unsupported) => {
                        return refuse(
                            Errc::Unsupported,
                            "an identifier outside the basic character set",
                        );
                    }
                    Err(e) => return refuse(e, "not a qualified name (§6.1 rule 1)"),
                };
                Answer::Values(
                    s.lookup(global, &components).into_iter().map(Value::Entity).collect(),
                )
            }
            wire::OP_KIND => {
                let v = self.value_arg(ints[0])?;
                let bits = s.kind_bits(v);
                let allowed = v.sort_bit()
                    | match v {
                        Value::Entity(_) => wire::KIND_ENTITY_BITS,
                        Value::Declaration(_) => wire::KIND_DEFINITION,
                        Value::Reference(_) => 0,
                        Value::Expression(_) => wire::KIND_CALL,
                    };
                if bits & v.sort_bit() == 0 || bits & !allowed != 0 {
                    return Err(Stop::Internal(format!(
                        "the subject gave kind bits {bits:#x} for {}",
                        v.sort_name()
                    )));
                }
                Answer::Word(bits)
            }
            wire::OP_IDENTIFIER_OF => {
                let v = self.value_arg(ints[0])?;
                if let Value::Expression(_) = v {
                    return Err(self.wrong_kind(v, "an entity, a declaration or a reference"));
                }
                match s.identifier(v) {
                    Some(id) => Answer::Bytes(id),
                    None => return Err(self.wrong_kind(v, "a value with a name")),
                }
            }
            wire::OP_DISPLAY_STRING_OF => {
                let v = self.value_arg(ints[0])?;
                Answer::Bytes(s.display(v).into_bytes())
            }
            wire::OP_PARENT_OF => {
                Answer::Info(s.parent(self.entity_arg(ints[0])?).map(Value::Entity))
            }
            wire::OP_PARAMETERS_OF => {
                let v = self.value_arg(ints[0])?;
                let expected = "a function, a function template or a declaration of one";
                if let Value::Reference(_) | Value::Expression(_) = v {
                    return Err(self.wrong_kind(v, expected));
                }
                match s.parameters(v) {
                    Some(p) => Answer::Values(p.into_iter().map(Value::Entity).collect()),
                    None => return Err(self.wrong_kind(v, expected)),
                }
            }
            wire::OP_DECLARATIONS_OF => {
                let e = self.entity_arg(ints[0])?;
                match s.declarations(e) {
                    Ok(d) => Answer::Values(d.into_iter().map(Value::Declaration).collect()),
                    Err(errc) => return Err(self.subject_refusal(Value::Entity(e), errc)),
                }
            }
            wire::OP_ENTITY_OF => match self.value_arg(ints[0])? {
                Value::Declaration(d) => Answer::Info(Some(Value::Entity(s.entity_of(d)))),
                v => return Err(self.wrong_kind(v, "a declaration")),
            },
            wire::OP_REFERENCES_TO => {
                let e = self.entity_arg(ints[0])?;
                Answer::Values(s.references_to(e).into_iter().map(Value::Reference).collect())
            }
            wire::OP_REFERENT_OF => match self.value_arg(ints[0])? {
                Value::Reference(r) => Answer::Info(Some(Value::Entity(s.referent(r)))),
                v => return Err(self.wrong_kind(v, "a reference")),
            },
            wire::OP_NAME_RANGE_OF => {
                let v = self.value_arg(ints[0])?;
                if let Value::Entity(_) | Value::Expression(_) = v {
                    return Err(self.wrong_kind(v, "a declaration or a reference"));
                }
                match s.name_range(v) {
                    Ok(r) => Answer::Range(r),
                    Err(errc) => return Err(self.subject_refusal(v, errc)),
                }
            }
            wire::OP_EXTENT_OF => {
                let v = self.value_arg(ints[0])?;
                match v {
                    Value::Entity(_) => {
                        return Err(self.wrong_kind(v, "a reference or an expression"));
                    }
                    Value::Declaration(_) => {
                        return Err(self.refusal_at(
                            v,
                            Errc::Unsupported,
                            "the extent of a declaration is not in this version".to_owned(),
                        ));
                    }
                    Value::Reference(_) | Value::Expression(_) => {}
                }
                match s.extent(v) {
                    Ok(r) => Answer::Range(r),
                    Err(errc) => return Err(self.subject_refusal(v, errc)),
                }
            }
            wire::OP_SOURCE_TEXT_OF => {
                let r = self.range_arg(ints[0], ints[1])?;
                let text = s.text(r);
                if text.len() as u64 != u64::from(r.hi - r.lo) {
                    return Err(Stop::Internal(format!(
                        "the subject gave {} bytes for a range of {}",
                        text.len(),
                        r.hi - r.lo
                    )));
                }
                Answer::Bytes(text.to_vec())
            }
            wire::OP_IS_EDITABLE => {
                let r = self.range_arg(ints[0], ints[1])?;
                Answer::Word(u64::from(s.owned(r.file)))
            }
            wire::OP_CALLS_TO => {
                let e = self.entity_arg(ints[0])?;
                match s.calls_to(e) {
                    Ok(x) => Answer::Values(x.into_iter().map(Value::Expression).collect()),
                    Err(errc) => return Err(self.subject_refusal(Value::Entity(e), errc)),
                }
            }
            wire::OP_CALLEE_OF | wire::OP_CALLEE_REFERENCE_OF | wire::OP_ARGUMENTS_AS_WRITTEN => {
                let v = self.value_arg(ints[0])?;
                let Value::Expression(x) = v else { return Err(self.wrong_kind(v, "a call")) };
                let answer = match row.op {
                    wire::OP_CALLEE_OF => s.callee(x).map(|e| Answer::Info(Some(Value::Entity(e)))),
                    wire::OP_CALLEE_REFERENCE_OF => {
                        s.callee_reference(x).map(|r| Answer::Info(Some(Value::Reference(r))))
                    }
                    _ => s
                        .arguments(x)
                        .map(|a| Answer::Values(a.into_iter().map(Value::Expression).collect())),
                };
                match answer {
                    Some(a) => a,
                    None => return Err(self.wrong_kind(v, "a call")),
                }
            }
            _ if row.effect => {
                return refuse(
                    Errc::Unsupported,
                    "effects are not served by this host yet (WP-088)",
                );
            }
            _ => unreachable!("operation {} is in the table and not dispatched", row.op),
        })
    }

    /// A refusal the subject chose (`Err(errc)` from a method).
    fn subject_refusal(&self, v: Value, errc: Errc) -> Stop {
        self.refusal_at(v, errc, format!("{} refused for {}", errc.name(), v.sort_name()))
    }

    // -----------------------------------------------------------------------
    // Delivery.

    /// Checks what the subject answered: every value live, every range in
    /// its file, no duplicate in a sequence (§5.3 rule 2), every count
    /// below 2^28 (refused with `limit_exceeded`). Changes nothing.
    fn check(&self, answer: Answer) -> Step<Answer> {
        let live = |v: Value| -> Step<()> {
            if self.subject.is_live(v) {
                Ok(())
            } else {
                Err(Stop::Internal(format!(
                    "the subject answered with {} that is not live",
                    v.sort_name()
                )))
            }
        };
        let too_many = |n: usize| -> Step<()> {
            if n as u64 >= wire::COUNT_LIMIT {
                refuse(
                    Errc::LimitExceeded,
                    format!("an answer of {n} items does not fit the result word"),
                )
            } else {
                Ok(())
            }
        };
        match &answer {
            Answer::Word(w) => {
                if *w > wire::PAYLOAD_MASK {
                    return Err(Stop::Internal(format!("value {w:#x} does not fit 56 bits")));
                }
            }
            Answer::Info(v) => {
                if let Some(v) = v {
                    live(*v)?;
                }
            }
            Answer::Values(vs) => {
                too_many(vs.len())?;
                let mut seen = HashSet::with_capacity(vs.len());
                for v in vs {
                    live(*v)?;
                    if !seen.insert(*v) {
                        return Err(Stop::Internal(format!(
                            "the subject answered with {} twice in one sequence",
                            v.sort_name()
                        )));
                    }
                }
            }
            Answer::Range(r) => {
                if let Some(r) = r
                    && !self.range_is_valid(*r)
                {
                    return Err(Stop::Internal(
                        "the subject gave a range outside its file".to_owned(),
                    ));
                }
            }
            Answer::Bytes(b) => too_many(b.len())?,
        }
        Ok(answer)
    }

    /// Issues the handles the answer needs, writes it (convention B: all
    /// or nothing) and returns the result word.
    fn deliver(&mut self, row: &Operation, answer: Answer, out: &mut dyn Out) -> u64 {
        let (ints, bytes) = match answer {
            Answer::Word(w) => {
                debug_assert_eq!(row.convention, wire::Convention::V, "{}", row.name);
                return wire::value_word(w);
            }
            Answer::Info(v) => {
                debug_assert_eq!(row.convention, wire::Convention::V, "{}", row.name);
                return wire::value_word(v.map_or(0, |v| self.issue(v)));
            }
            Answer::Values(vs) => (vs.into_iter().map(|v| self.issue(v)).collect(), Vec::new()),
            Answer::Range(None) => (Vec::new(), Vec::new()),
            Answer::Range(Some(r)) => {
                let f = u64::from(self.issue_file(r.file));
                (vec![f, u64::from(r.lo) << 32 | u64::from(r.hi)], Vec::new())
            }
            Answer::Bytes(b) => (Vec::new(), b),
        };
        debug_assert_eq!(row.convention, wire::Convention::B, "{}", row.name);
        if ints.len() <= out.int_capacity() && bytes.len() <= out.byte_capacity() {
            if !ints.is_empty() {
                out.write_ints(&ints);
            }
            if !bytes.is_empty() {
                out.write_bytes(&bytes);
            }
        }
        wire::counts_word(bytes.len() as u64, ints.len() as u64)
    }

    /// The handle of live value `v`, issuing one the first time.
    pub(crate) fn issue(&mut self, v: Value) -> u64 {
        let sort = v.tag() as usize - 1;
        let n = v.number();
        let issued = &mut self.issued[sort];
        let index = *self.index_of[sort].entry(n).or_insert_with(|| {
            issued.push(n);
            issued.len() as u64 - 1
        });
        wire::handle(v.tag(), index)
    }

    /// The file handle of live file `f`, issuing one the first time.
    pub(crate) fn issue_file(&mut self, f: File) -> u32 {
        let files = &mut self.files;
        *self.file_handle.entry(f).or_insert_with(|| {
            files.push(f);
            files.len() as u32
        })
    }
}

/// Parses a qualified name by the grammar of §6.1 rule 1: `::`, or an
/// optional leading `::` followed by identifiers separated by `::`.
/// Returns whether it began with `::` and the identifiers (`(true, [])` for
/// `::` alone).
///
/// `Err(InvalidArgument)` for anything else; `Err(Unsupported)` for a name
/// that is otherwise well formed and has a byte outside ASCII in an
/// identifier (`spec/landed/api/WP-081.md`).
pub fn parse_qualified_name(text: &[u8]) -> Result<(bool, Vec<&[u8]>), Errc> {
    if text == b"::" {
        return Ok((true, Vec::new()));
    }
    let (global, mut rest) = match text.strip_prefix(b"::") {
        Some(r) => (true, r),
        None => (false, text),
    };
    let mut components = Vec::new();
    while let Some(i) = rest.windows(2).position(|w| w == b"::") {
        components.push(&rest[..i]);
        rest = &rest[i + 2..];
    }
    components.push(rest);
    let mut unsupported = false;
    for c in &components {
        match check_identifier(c) {
            Ok(()) => {}
            Err(Errc::Unsupported) => unsupported = true,
            Err(e) => return Err(e),
        }
    }
    if unsupported { Err(Errc::Unsupported) } else { Ok((global, components)) }
}

/// Whether `id` is an identifier (§6.1 rule 1, §7.5 rule 3): a letter or
/// `_`, then letters, digits and `_`. `Err(InvalidArgument)` if it is not;
/// `Err(Unsupported)` if it would be one but for bytes outside ASCII,
/// which this version does not classify.
pub fn check_identifier(id: &[u8]) -> Result<(), Errc> {
    let Some(&first) = id.first() else { return Err(Errc::InvalidArgument) };
    if first.is_ascii_digit() {
        return Err(Errc::InvalidArgument);
    }
    let mut non_ascii = false;
    for &b in id {
        if b >= 0x80 {
            non_ascii = true;
        } else if !(b.is_ascii_alphanumeric() || b == b'_') {
            return Err(Errc::InvalidArgument);
        }
    }
    if non_ascii { Err(Errc::Unsupported) } else { Ok(()) }
}
