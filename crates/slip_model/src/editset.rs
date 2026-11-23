//! The edit set, kind `EDTS` (`spec/edits.md` §6): what reduce decided
//! from the claims of every observer. Work package WP-061.
//!
//! Plain data, [`basis_id`], and the geometry of `spec/edits.md` §5.4
//! ([`share_byte`], [`within`], [`properly_inside`], [`cross`]): the one
//! definition the format's reader, reduce and apply use. The binary and
//! JSON forms and the invariants (§6.1) are in `formats`
//! (`spec/landed/edits/WP-061.md`).

use crate::edits::{Failure, KeyId, MacroSite, Piece, Place, Reason, Report, Subject};
use crate::hash::{BASIS_TAG, tagged_digest};
use crate::keys::Rule;
use crate::wire::Encoder;
use crate::{Counter, Digest, FileRef};

/// The scheme of the edit set this crate reads and writes (§6).
pub const SCHEME: u64 = 1;

/// The most entries of a `first` list and of an edit's `notes` (§6, §5.3).
pub const FIRST_MAX: usize = 8;

/// One shard an edit set was reduced from (§6): the observer and the
/// content id of its shard, absent if it is unparsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Basis {
    pub observer: Digest,
    pub shard: Option<Digest>,
}

/// `H("slip.basis.v1", list<Basis>)` (`spec/model.md` §3, `spec/edits.md`
/// §6): `basis` in plan order, each `observer: digest, shard:
/// opt<digest>`.
pub fn basis_id(basis: &[Basis]) -> Digest {
    let mut e = Encoder::new();
    e.count(basis.len());
    for b in basis {
        e.digest(&b.observer);
        match &b.shard {
            None => e.u8(0),
            Some(s) => e.u8(1).digest(s),
        };
    }
    tagged_digest(BASIS_TAG, e.as_bytes())
}

/// Whether reduce accepted the edits (§6, §5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetState {
    Accepted = 0,
    Refused = 1,
}

/// An accepted edit: a collapsed `replace` claim (§5.3, §6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edit {
    /// Index into the set's `files`.
    pub file: u64,
    pub lo: u64,
    /// `lo < hi <=` the file's size.
    pub hi: u64,
    /// A copy's file is an index of the set's `files`.
    pub pieces: Vec<Piece>,
    /// Sorted, no two equal.
    pub subjects: Vec<KeyId>,
    /// `edits::tiers`: tier 0 or 1 bits, not 0.
    pub tiers: u64,
    /// Absent if `tiers` is tier 0 alone. `macro` in the spec.
    pub macro_site: Option<MacroSite>,
    /// Sorted, no two equal, at most [`FIRST_MAX`].
    pub notes: Vec<Vec<u8>>,
    /// The observers that made the claim.
    pub observers: u64,
    pub votes: u64,
    /// The observers of the least 8 indices that made the claim, in plan order.
    pub first: Vec<Digest>,
}

/// A joined `changed` entry (§4.5, §6.2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Exempt {
    pub key: KeyId,
    /// `lo < hi`, within its file.
    pub place: Place,
    pub reason: Vec<u8>,
    /// An accepted edit shares a byte with `place` (§6.2).
    pub in_force: bool,
    pub observers: u64,
    pub first: Vec<Digest>,
}

/// A joined report (§4.5): its place is in the set's `files`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reported {
    pub report: Report,
    pub observers: u64,
    pub first: Vec<Digest>,
}

/// A joined failure (§4.6): `rendered` and `refusal` those of the first observer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Failed {
    pub failure: Failure,
    pub observers: u64,
    pub first: Vec<Digest>,
}

/// Why something was refused (§5.2, §5.4, §5.5, §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cause {
    NoObserver = 0,
    NoSubject = 1,
    SubjectReadOnly = 2,
    SubjectUnsupported = 3,
    SubjectUnreachable = 4,
    NameTaken = 5,
    MixedFollower = 6,
    MixedSet = 7,
    SiteRefused = 8,
    Contested = 9,
    Divergent = 10,
    Overlap = 11,
    FileDiffers = 12,
    Cyclic = 13,
    Buried = 14,
    RuleConflict = 15,
    NoEdit = 16,
}

impl Cause {
    /// Every value, in code order.
    pub const ALL: [Cause; 17] = [
        Cause::NoObserver,
        Cause::NoSubject,
        Cause::SubjectReadOnly,
        Cause::SubjectUnsupported,
        Cause::SubjectUnreachable,
        Cause::NameTaken,
        Cause::MixedFollower,
        Cause::MixedSet,
        Cause::SiteRefused,
        Cause::Contested,
        Cause::Divergent,
        Cause::Overlap,
        Cause::FileDiffers,
        Cause::Cyclic,
        Cause::Buried,
        Cause::RuleConflict,
        Cause::NoEdit,
    ];

    /// A refusal of a subject (§5.2 rule 3): `no_subject` to `mixed_follower`.
    pub fn is_of_subject(self) -> bool {
        matches!(
            self,
            Cause::NoSubject
                | Cause::SubjectReadOnly
                | Cause::SubjectUnsupported
                | Cause::SubjectUnreachable
                | Cause::NameTaken
                | Cause::MixedFollower
        )
    }
}

/// A refusal (§6). Sorted by `(cause, file, lo, hi, subject)`, an absent
/// `file` or `subject` first ([`Refusal::cmp_key`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Refusal {
    pub cause: Cause,
    /// Absent for a refusal that is not of a site.
    pub file: Option<u64>,
    /// 0 when `file` is absent.
    pub lo: u64,
    /// 0 when `file` is absent.
    pub hi: u64,
    /// For `site_refused`; else `none`.
    pub reason: Reason,
    pub subject: Option<KeyId>,
    /// `macro` in the spec.
    pub macro_site: Option<MacroSite>,
    /// For `contested` by another range, `overlap` and `buried`: the least
    /// other range involved.
    pub other: Option<Place>,
    pub observers: u64,
    pub first: Vec<Digest>,
}

impl Refusal {
    /// The sort key of §6, `(cause, file, lo, hi, subject)`, an absent
    /// `file` or `subject` before any present one.
    pub fn cmp_key(&self, other: &Refusal) -> std::cmp::Ordering {
        (self.cause, self.file, self.lo, self.hi, self.subject).cmp(&(
            other.cause,
            other.file,
            other.lo,
            other.hi,
            other.subject,
        ))
    }
}

/// The edit set (§6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditSet {
    /// [`SCHEME`].
    pub scheme: u64,
    pub plan: Digest,
    pub round: u64,
    /// The request id.
    pub transform: Digest,
    /// [`basis_id`] of the shards reduced; not checked by the format.
    pub basis: Digest,
    pub state: SetState,
    /// Canonical real paths; sorted by path, no two equal.
    pub files: Vec<FileRef>,
    /// Sorted by id, no two equal.
    pub subjects: Vec<Subject>,
    /// A rule table (`spec/verify.md` §2).
    pub rules: Vec<Rule>,
    /// Sorted by `(file, lo, hi)`, no two with one range, disjoint or
    /// nested; empty unless accepted.
    pub edits: Vec<Edit>,
    /// Atomic groups of two or more of `edits` (§4.4).
    pub groups: Vec<Vec<u64>>,
    /// Sorted by `(key, place, reason)`, no two equal.
    pub changed: Vec<Exempt>,
    /// Sorted, no two equal.
    pub kept: Vec<Place>,
    /// Sorted, no two equal.
    pub unresolved: Vec<Place>,
    /// Sorted by [`Refusal::cmp_key`], no two equal in it.
    pub refusals: Vec<Refusal>,
    /// Sorted by the report, no two equal.
    pub reports: Vec<Reported>,
    /// Sorted by `(kind, message, place)`, no two equal in that key.
    pub failures: Vec<Failed>,
    /// Observers without an `ok` shard; in plan order.
    pub unparsed: Vec<Digest>,
    /// §6.3; sorted by name.
    pub counters: Vec<Counter>,
}

// ---------------------------------------------------------------------------
// Geometry (spec/edits.md §5.4)
//
// A range `(lo, hi)` is the bytes `lo..hi` of one file. A range with
// `lo >= hi` has no byte: it shares a byte with no range, is within no
// range, no range is within it, and it crosses none. So for any two
// ranges exactly one holds: they are disjoint (share no byte), one is
// within the other, or they cross (`spec/landed/edits/WP-061.md` §1).

/// `a` and `b` have a byte in common.
pub fn share_byte(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}

/// `a` is `b`, or inside it: every byte of `a` is a byte of `b`, and `a`
/// has a byte.
pub fn within(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < a.1 && b.0 <= a.0 && a.1 <= b.1
}

/// `a` is within `b` and is not `b`.
pub fn properly_inside(a: (u64, u64), b: (u64, u64)) -> bool {
    within(a, b) && a != b
}

/// `a` and `b` share a byte and neither is within the other.
pub fn cross(a: (u64, u64), b: (u64, u64)) -> bool {
    share_byte(a, b) && !within(a, b) && !within(b, a)
}
