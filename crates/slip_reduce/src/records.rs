//! The records reduce collapses (`spec/edits.md` §5.3, §5.6): one key per
//! thing two observers can say alike, and the join of what they said.
//!
//! Every place is by path, never by a shard's file index, so that records
//! of different shards compare. The order of [`Key`] is the order of the
//! temporary files and of the final stream: files first (sorted by path, so
//! that a path's rank is its index in the edit set), then claims by site,
//! then reports, `changed` entries, instances and failures, each in the
//! order the edit set lists them.

use slip_model::Digest;
use slip_model::edits::{FailureKind, MacroSite, Reason, Severity, Stance};
use slip_model::editset::FIRST_MAX;
use slip_model::wire::{Decoder, Encoder};

/// The most notes a collapsed claim keeps (§5.3).
pub(crate) const NOTES_MAX: usize = 8;

/// A range of a file, the file by path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Site {
    pub path: Vec<u8>,
    pub lo: u64,
    pub hi: u64,
}

/// A piece with its copy's file by path (§5.3: "a copy piece compared by
/// its file's path and its range"). The variant order is the tag order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum KPiece {
    Literal(Vec<u8>),
    Copy(Site),
    Spelled { key: Digest, text: Vec<u8> },
    Identifier { text: Vec<u8>, key: Option<Digest> },
}

/// One record of a site.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Entry {
    /// The claims of one stance and pieces (§5.3).
    Claim { stance: Stance, pieces: Vec<KPiece> },
    /// The [`variant`]s of the claims one observer made at the site, when
    /// it made two or more: with [`ClaimVal::single`], what counts each
    /// observer once over several claims of a site (landed WP-062 §4).
    Observed(Vec<Digest>),
}

/// What a record is about; two records with one key are joined.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    /// An observer of the plan, with a sample of the indices that name it:
    /// first in the order, so that the merged stream opens with them and a
    /// plan that names one observer twice is seen before any claim
    /// (landed WP-062 §2).
    Observer(Digest),
    /// A file some shard's `edits.files` names, with the size and content
    /// that shard gave it.
    File { path: Vec<u8>, size: u64, content: Digest },
    /// A claim (§5.3), or what one observer claimed at a site where it made
    /// more than one claim: by site, so that a site's records are adjacent.
    Claim { site: Site, entry: Entry },
    /// A report, by its whole record (§5.3); ordered as §4.5 orders reports.
    Report { place: Option<Site>, severity: Severity, message: Vec<u8>, attached: bool },
    /// A `changed` entry, by its whole record.
    Changed { key: Digest, place: Site, reason: Vec<u8> },
    /// An instance (§4.5, WP-139), by its whole record.
    Instance { place: Site, spec: Option<Digest>, display: String, subjects: Vec<Digest> },
    /// A failure, by `(kind, message, place)` (§6).
    Failure { kind: FailureKind, message: String, place: Option<(Vec<u8>, u64, u64)> },
}

/// How many observers said a thing, and the least eight of them (§5.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sample {
    pub observers: u64,
    /// Observer indices, ascending, at most [`FIRST_MAX`].
    pub first: Vec<u64>,
}

impl Sample {
    pub fn one(index: u64) -> Sample {
        Sample { observers: 1, first: vec![index] }
    }

    /// The join: counts add, `first` is the least eight of the union.
    pub fn join(&mut self, other: &Sample) {
        self.observers = self.observers.saturating_add(other.observers);
        self.first = least_union(&self.first, &other.first, FIRST_MAX);
    }
}

/// The least `max` elements of the union of two ascending lists.
#[allow(clippy::many_single_char_names)]
pub(crate) fn least_union<T: Ord + Clone>(a: &[T], b: &[T], max: usize) -> Vec<T> {
    let mut out = Vec::with_capacity((a.len() + b.len()).min(max));
    let (mut i, mut j) = (0, 0);
    while out.len() < max && (i < a.len() || j < b.len()) {
        let next = match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x < y => {
                i += 1;
                x
            }
            (Some(x), Some(y)) if x > y => {
                j += 1;
                y
            }
            (Some(x), Some(_)) => {
                i += 1;
                j += 1;
                x
            }
            (Some(x), None) => {
                i += 1;
                x
            }
            (None, Some(y)) => {
                j += 1;
                y
            }
            (None, None) => break,
        };
        out.push(next.clone());
    }
    out
}

/// The union of two ascending lists.
pub(crate) fn union<T: Ord + Clone>(a: &[T], b: &[T]) -> Vec<T> {
    least_union(a, b, usize::MAX)
}

/// The join of claims with one key (§5.3's table).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClaimVal {
    /// Ascending, no two equal.
    pub subjects: Vec<Digest>,
    pub tiers: u64,
    pub macro_site: Option<MacroSite>,
    pub reason: Reason,
    /// Ascending, no two equal, at most [`NOTES_MAX`].
    pub notes: Vec<Vec<u8>>,
    pub votes: u64,
    pub sample: Sample,
    /// The observers that made this claim and no other at the site.
    pub single: Sample,
}

impl ClaimVal {
    pub fn join(&mut self, other: &ClaimVal) {
        self.subjects = union(&self.subjects, &other.subjects);
        self.tiers |= other.tiers;
        self.macro_site = least_macro(self.macro_site.take(), other.macro_site.clone());
        self.reason = least_reason(self.reason, other.reason);
        self.notes = least_union(&self.notes, &other.notes, NOTES_MAX);
        self.votes = self.votes.saturating_add(other.votes);
        self.sample.join(&other.sample);
        self.single.join(&other.single);
    }
}

/// The least `MacroSite` (§4.3), absent if neither has one.
pub(crate) fn least_macro(a: Option<MacroSite>, b: Option<MacroSite>) -> Option<MacroSite> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The least reason other than `none` (§4.3).
pub(crate) fn least_reason(a: Reason, b: Reason) -> Reason {
    match (a, b) {
        (Reason::None, r) | (r, Reason::None) => r,
        (a, b) => a.min(b),
    }
}

/// A failure's host refusal with its subject's file by path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefusalK {
    pub function: String,
    pub errc: String,
    pub message: String,
    pub subject: Option<Site>,
}

/// The join of failures with one key: `rendered` and `refusal` those of
/// the least observer index (§6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FailVal {
    pub sample: Sample,
    pub from: u64,
    pub rendered: String,
    pub refusal: Option<RefusalK>,
}

/// What is joined under a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Val {
    File,
    Claim(ClaimVal),
    Sample(Sample),
    Failure(FailVal),
}

impl Val {
    /// The join of two values of one key. Values of one key are of one
    /// variant by construction; a mismatch is a corrupt temporary file.
    pub fn join(&mut self, other: &Val) -> Result<(), String> {
        match (self, other) {
            (Val::File, Val::File) => {}
            (Val::Claim(a), Val::Claim(b)) => a.join(b),
            (Val::Sample(a), Val::Sample(b)) => a.join(b),
            (Val::Failure(a), Val::Failure(b)) => {
                a.sample.join(&b.sample);
                if b.from < a.from {
                    a.from = b.from;
                    a.rendered.clone_from(&b.rendered);
                    a.refusal.clone_from(&b.refusal);
                }
            }
            _ => return Err("a temporary file joins records of different kinds".into()),
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The codec of the temporary files. Private to one call of `reduce`; not a
// container and not specified (docs/wp/WP-062.md).

fn site(e: &mut Encoder, s: &Site) {
    e.bytes(&s.path).uv(s.lo).uv(s.hi);
}

fn opt_site(e: &mut Encoder, s: Option<&Site>) {
    match s {
        None => e.u8(0),
        Some(s) => {
            e.u8(1);
            site(e, s);
            e
        }
    };
}

fn sample(e: &mut Encoder, s: &Sample) {
    e.uv(s.observers).count(s.first.len());
    for &i in &s.first {
        e.uv(i);
    }
}

fn stance_code(s: Stance) -> u64 {
    s as u64
}

/// Encodes one record.
#[allow(clippy::too_many_lines, reason = "one arm per record kind")]
pub(crate) fn encode(key: &Key, val: &Val) -> Vec<u8> {
    let mut e = Encoder::new();
    match key {
        Key::Observer(o) => {
            e.u8(5).digest(o);
        }
        Key::File { path, size, content } => {
            e.u8(0).bytes(path).uv(*size).digest(content);
        }
        Key::Claim { site: s, entry: Entry::Observed(variants) } => {
            e.u8(1);
            site(&mut e, s);
            e.u8(1).count(variants.len());
            for v in variants {
                e.digest(v);
            }
        }
        Key::Claim { site: s, entry: Entry::Claim { stance, pieces } } => {
            e.u8(1);
            site(&mut e, s);
            e.u8(0).uv(stance_code(*stance)).count(pieces.len());
            for p in pieces {
                match p {
                    KPiece::Literal(b) => e.u8(0).bytes(b),
                    KPiece::Copy(s) => {
                        e.u8(1);
                        site(&mut e, s);
                        &mut e
                    }
                    KPiece::Spelled { key, text } => e.u8(2).digest(key).bytes(text),
                    KPiece::Identifier { text, key } => {
                        e.u8(3).bytes(text);
                        match key {
                            None => e.u8(0),
                            Some(k) => e.u8(1).digest(k),
                        }
                    }
                };
            }
        }
        Key::Report { place, severity, message, attached } => {
            e.u8(2);
            opt_site(&mut e, place.as_ref());
            e.uv(*severity as u64).bytes(message).bool(*attached);
        }
        Key::Changed { key, place, reason } => {
            e.u8(3).digest(key);
            site(&mut e, place);
            e.bytes(reason);
        }
        Key::Instance { place, spec, display, subjects } => {
            e.u8(6);
            site(&mut e, place);
            match spec {
                None => e.u8(0),
                Some(k) => e.u8(1).digest(k),
            };
            e.str(display).count(subjects.len());
            for k in subjects {
                e.digest(k);
            }
        }
        Key::Failure { kind, message, place } => {
            e.u8(4).uv(*kind as u64).str(message);
            match place {
                None => e.u8(0),
                Some((f, l, c)) => e.u8(1).bytes(f).uv(*l).uv(*c),
            };
        }
    }
    match val {
        Val::File => {}
        Val::Claim(c) => {
            e.count(c.subjects.len());
            for s in &c.subjects {
                e.digest(s);
            }
            e.uv(c.tiers);
            match &c.macro_site {
                None => e.u8(0),
                Some(m) => e.u8(1).uv(m.name_start).bytes(&m.name),
            };
            e.uv(c.reason as u64).count(c.notes.len());
            for n in &c.notes {
                e.bytes(n);
            }
            e.uv(c.votes);
            sample(&mut e, &c.sample);
            sample(&mut e, &c.single);
        }
        Val::Sample(s) => sample(&mut e, s),
        Val::Failure(f) => {
            sample(&mut e, &f.sample);
            e.uv(f.from).str(&f.rendered);
            match &f.refusal {
                None => e.u8(0),
                Some(r) => {
                    e.u8(1).str(&r.function).str(&r.errc).str(&r.message);
                    opt_site(&mut e, r.subject.as_ref());
                    &mut e
                }
            };
        }
    }
    e.into_bytes()
}

type R<T> = Result<T, String>;

fn bad(what: &str) -> String {
    format!("a temporary file of reduce is corrupt: {what}")
}

fn w<T>(r: Result<T, slip_model::wire::Error>) -> R<T> {
    r.map_err(|e| bad(&e.to_string()))
}

fn dsite(d: &mut Decoder<'_>) -> R<Site> {
    Ok(Site { path: w(d.bytes())?.to_vec(), lo: w(d.uv())?, hi: w(d.uv())? })
}

fn dopt_site(d: &mut Decoder<'_>) -> R<Option<Site>> {
    match w(d.u8())? {
        0 => Ok(None),
        1 => Ok(Some(dsite(d)?)),
        _ => Err(bad("an option tag")),
    }
}

fn dsample(d: &mut Decoder<'_>) -> R<Sample> {
    let observers = w(d.uv())?;
    let n = w(d.count())?;
    let mut first = Vec::with_capacity(n.min(FIRST_MAX));
    for _ in 0..n {
        first.push(w(d.uv())?);
    }
    Ok(Sample { observers, first })
}

pub(crate) fn stance_of(v: u64) -> Option<Stance> {
    [Stance::Replace, Stance::Keep, Stance::Refuse, Stance::Unresolved].get(v as usize).copied()
}

pub(crate) fn reason_of(v: u64) -> Option<Reason> {
    [
        Reason::None,
        Reason::MacroBody,
        Reason::Pasted,
        Reason::Crossing,
        Reason::NotAFile,
        Reason::NotOwned,
        Reason::FileRecoded,
        Reason::NotPlain,
        Reason::MixedSet,
        Reason::MixedInstances,
        Reason::Directive,
        Reason::Instantiated,
    ]
    .get(v as usize)
    .copied()
    // 12 is unlisted (`spec/edits.md` §4.2); `rebuilt` is 13 (WP-118),
    // `captured_at_instance` is 14 (WP-152).
    .or(match v {
        13 => Some(Reason::Rebuilt),
        14 => Some(Reason::CapturedAtInstance),
        _ => None,
    })
}

fn severity_of(v: u64) -> Option<Severity> {
    [Severity::Note, Severity::Review, Severity::Error].get(v as usize).copied()
}

fn failure_kind_of(v: u64) -> Option<FailureKind> {
    [
        FailureKind::Uncaught,
        FailureKind::NotConstant,
        FailureKind::StepBudget,
        FailureKind::MemoryBudget,
        FailureKind::DepthLimit,
        FailureKind::HostFailure,
    ]
    .get(v as usize)
    .copied()
}

/// Decodes one record written by [`encode`].
#[allow(clippy::too_many_lines)]
pub(crate) fn decode(bytes: &[u8]) -> R<(Key, Val)> {
    let mut d = Decoder::new(bytes);
    let tag = w(d.u8())?;
    let key = match tag {
        0 => Key::File { path: w(d.bytes())?.to_vec(), size: w(d.uv())?, content: w(d.digest())? },
        1 => {
            let s = dsite(&mut d)?;
            if w(d.u8())? == 1 {
                let n = w(d.count())?;
                let mut variants = Vec::with_capacity(n);
                for _ in 0..n {
                    variants.push(w(d.digest())?);
                }
                let val = Val::Sample(dsample(&mut d)?);
                w(d.finish())?;
                return Ok((Key::Claim { site: s, entry: Entry::Observed(variants) }, val));
            }
            let stance = stance_of(w(d.uv())?).ok_or_else(|| bad("a stance"))?;
            let n = w(d.count())?;
            let mut pieces = Vec::with_capacity(n);
            for _ in 0..n {
                pieces.push(match w(d.u8())? {
                    0 => KPiece::Literal(w(d.bytes())?.to_vec()),
                    1 => KPiece::Copy(dsite(&mut d)?),
                    2 => KPiece::Spelled { key: w(d.digest())?, text: w(d.bytes())?.to_vec() },
                    3 => KPiece::Identifier {
                        text: w(d.bytes())?.to_vec(),
                        key: match w(d.u8())? {
                            0 => None,
                            1 => Some(w(d.digest())?),
                            _ => return Err(bad("an option tag")),
                        },
                    },
                    _ => return Err(bad("a piece tag")),
                });
            }
            Key::Claim { site: s, entry: Entry::Claim { stance, pieces } }
        }
        2 => Key::Report {
            place: dopt_site(&mut d)?,
            severity: severity_of(w(d.uv())?).ok_or_else(|| bad("a severity"))?,
            message: w(d.bytes())?.to_vec(),
            attached: w(d.bool())?,
        },
        3 => Key::Changed {
            key: w(d.digest())?,
            place: dsite(&mut d)?,
            reason: w(d.bytes())?.to_vec(),
        },
        4 => Key::Failure {
            kind: failure_kind_of(w(d.uv())?).ok_or_else(|| bad("a failure kind"))?,
            message: w(d.str())?.to_owned(),
            place: match w(d.u8())? {
                0 => None,
                1 => Some((w(d.bytes())?.to_vec(), w(d.uv())?, w(d.uv())?)),
                _ => return Err(bad("an option tag")),
            },
        },
        5 => Key::Observer(w(d.digest())?),
        6 => {
            let place = dsite(&mut d)?;
            let spec = match w(d.u8())? {
                0 => None,
                1 => Some(w(d.digest())?),
                _ => return Err(bad("an option tag")),
            };
            let display = w(d.str())?.to_owned();
            let n = w(d.count())?;
            let mut subjects = Vec::with_capacity(n);
            for _ in 0..n {
                subjects.push(w(d.digest())?);
            }
            Key::Instance { place, spec, display, subjects }
        }
        _ => return Err(bad("a record tag")),
    };
    let val = match tag {
        0 => Val::File,
        1 => {
            let n = w(d.count())?;
            let mut subjects = Vec::with_capacity(n);
            for _ in 0..n {
                subjects.push(w(d.digest())?);
            }
            let tiers = w(d.uv())?;
            let macro_site = match w(d.u8())? {
                0 => None,
                1 => Some(MacroSite { name_start: w(d.uv())?, name: w(d.bytes())?.to_vec() }),
                _ => return Err(bad("an option tag")),
            };
            let reason = reason_of(w(d.uv())?).ok_or_else(|| bad("a reason"))?;
            let n = w(d.count())?;
            let mut notes = Vec::with_capacity(n);
            for _ in 0..n {
                notes.push(w(d.bytes())?.to_vec());
            }
            let votes = w(d.uv())?;
            let sample = dsample(&mut d)?;
            let single = dsample(&mut d)?;
            Val::Claim(ClaimVal {
                subjects,
                tiers,
                macro_site,
                reason,
                notes,
                votes,
                sample,
                single,
            })
        }
        2 | 3 | 5 | 6 => Val::Sample(dsample(&mut d)?),
        _ => {
            let sample = dsample(&mut d)?;
            let from = w(d.uv())?;
            let rendered = w(d.str())?.to_owned();
            let refusal = match w(d.u8())? {
                0 => None,
                1 => Some(RefusalK {
                    function: w(d.str())?.to_owned(),
                    errc: w(d.str())?.to_owned(),
                    message: w(d.str())?.to_owned(),
                    subject: dopt_site(&mut d)?,
                }),
                _ => return Err(bad("an option tag")),
            };
            Val::Failure(FailVal { sample, from, rendered, refusal })
        }
    };
    w(d.finish())?;
    Ok((key, val))
}

/// The identity of an edit across shards (§4.2): `(file path, lo, hi,
/// pieces)`, a copy's file by path, as a digest. Group ties are kept by it
/// (docs/wp/WP-062.md "Group ties in pass 1").
pub(crate) fn identity(site: &Site, pieces: &[KPiece]) -> Digest {
    let entry = Entry::Claim { stance: Stance::Replace, pieces: pieces.to_vec() };
    let bytes = encode(&Key::Claim { site: site.clone(), entry }, &Val::File);
    slip_model::hash::tagged_digest("slip.reduce.edit.v1", &bytes)
}

/// A claim's stance and pieces at a site, as a digest: what
/// [`Entry::Observed`] lists.
pub(crate) fn variant(site: &Site, stance: Stance, pieces: &[KPiece]) -> Digest {
    let entry = Entry::Claim { stance, pieces: pieces.to_vec() };
    let bytes = encode(&Key::Claim { site: site.clone(), entry }, &Val::File);
    slip_model::hash::tagged_digest("slip.reduce.claim.v1", &bytes)
}
