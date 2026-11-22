//! The `edits` section, id 17 of a map shard (`spec/edits.md` §4): what
//! one run of a transform over one observer produced. Work package WP-060.
//!
//! Plain data, and [`Edits::normalise`], which a producer calls to put
//! its records in the form §4.7 requires. The binary and JSON forms and
//! the invariants are in `formats` (`spec/landed/edits/WP-060.md`).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::keys::Rule;
use crate::{Digest, FileRef};

/// An entity's key id in an observer (`spec/digest.md` §3).
pub type KeyId = Digest;

/// The scheme of the section this crate reads and writes (§4).
pub const SCHEME: u64 = 1;

/// A range of one of the section's `files` (§4): `file` is an index into
/// them; `lo <= hi <=` the file's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    pub file: u64,
    pub lo: u64,
    pub hi: u64,
}

/// What a subject is to the transform (§4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// The request names it (§9.1).
    Seed = 0,
    /// It must change if an entity it is linked to does (§9.2).
    Candidate = 1,
    /// Its key changes because of the entities in `of` (§9.2).
    Follower = 2,
}

/// The bits of `Subject::flags` (§4.1). Bits above 3 are 0.
pub mod subject_flags {
    /// Some declaration of the entity is in a read-only file.
    pub const READ_ONLY: u64 = 1;
    /// The transform cannot change an entity of this kind (§9.5).
    pub const UNSUPPORTED: u64 = 2;
    /// The scope already declares the new name (§9.5).
    pub const NAME_TAKEN: u64 = 4;
    /// The entity is referenced where the transform cannot find the name (§9.5).
    pub const UNREACHABLE: u64 = 8;
    /// Every bit that may be set.
    pub const ALL: u64 = 15;
}

/// An entity the transform is about, as far as this observer sees it (§4.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Subject {
    /// The entity's key id in this observer, with no rules.
    pub id: KeyId,
    /// The rule that undoes what the transform does to the entity's key
    /// (`spec/verify.md` §2.1); absent if its key does not change.
    pub rule: Option<Rule>,
    pub role: Role,
    /// For a follower: the entities it follows; sorted, not empty. Else empty.
    pub of: Vec<KeyId>,
    /// [`subject_flags`].
    pub flags: u64,
    /// The entity as a person would write it; for reports only.
    pub display: String,
}

/// Two subjects that change together or not at all (§4.1): `a < b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Link {
    pub a: KeyId,
    pub b: KeyId,
}

/// What a claim says of its bytes (§4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stance {
    /// These bytes become the text of `pieces`.
    Replace = 0,
    /// These bytes denote something the transform must not change.
    Keep = 1,
    /// These bytes would have to change and cannot.
    Refuse = 2,
    /// These bytes may denote a subject; this observer cannot tell.
    Unresolved = 3,
}

/// A part of a replacement (§4.2, `spec/api.md` §7.1). Tags 0, 1, 2.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Piece {
    /// Its bytes; not empty.
    Literal(Vec<u8>),
    /// The text of the range in the result of the run; `lo < hi`.
    Copy(Place),
    /// `text`, written as a name meant to denote the entity `key`; `text`
    /// not empty.
    Spelled { key: KeyId, text: Vec<u8> },
}

/// Why a claim refuses (§2.3, §4.2); `None` unless the stance is `Refuse`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    None = 0,
    MacroBody = 1,
    Pasted = 2,
    Crossing = 3,
    NotAFile = 4,
    NotOwned = 5,
    FileRecoded = 6,
    NotPlain = 7,
    MixedSet = 8,
    MixedInstances = 9,
}

/// The bits of `Claim::tiers` (§2.2, §4.2): bit t for tier t, bit 4 for X.
pub mod tiers {
    pub const T0: u64 = 1;
    pub const T1: u64 = 2;
    pub const T2: u64 = 4;
    pub const T3: u64 = 8;
    pub const X: u64 = 16;
    /// Every bit that may be set.
    pub const ALL: u64 = 31;
}

/// The outermost macro invocation a claim's bytes are an argument of or
/// were produced by (§4.2).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacroSite {
    /// Offset, in the claim's file, of the invocation's name token.
    pub name_start: u64,
    /// That macro's name.
    pub name: Vec<u8>,
}

/// A claim on bytes of a file (§4.2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Claim {
    /// Index into the section's `files`.
    pub file: u64,
    pub lo: u64,
    /// `lo <= hi <=` the file's size; `lo < hi` for `Replace`.
    pub hi: u64,
    pub stance: Stance,
    /// The replacement; empty unless the stance is `Replace`.
    pub pieces: Vec<Piece>,
    /// The subjects the bytes denote here; sorted, no two equal.
    pub subjects: Vec<KeyId>,
    pub reason: Reason,
    /// [`tiers`]: the tiers the claim was derived at.
    pub tiers: u64,
    /// Absent if `tiers` is bit 0 alone. `macro` in the spec.
    pub macro_site: Option<MacroSite>,
    /// Times this observer derived the claim; at least 1.
    pub votes: u64,
    /// Shown beside the edit in reports; sorted, no two equal.
    pub notes: Vec<Vec<u8>>,
}

/// How much a report matters (§4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Note = 0,
    Review = 1,
    Error = 2,
}

/// A report of the run (§4.5). The derived order is the section's:
/// `(place, severity, message, attached)`, an absent place first.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Report {
    /// Absent: the subject unit as a whole.
    pub place: Option<Place>,
    pub severity: Severity,
    pub message: Vec<u8>,
    /// The place is the nearest the producer could name, not the place
    /// meant (`spec/api.md` §7.4 rule 3).
    pub attached: bool,
}

/// A unit whose digest is meant to differ (§4.5). The derived order is the
/// section's: `(key, place, reason)`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Changed {
    pub key: KeyId,
    /// `lo < hi`: the bytes whose edit is the reason.
    pub place: Place,
    pub reason: Vec<u8>,
}

/// How a run of a programmable transform failed (§4.6, `spec/api.md` §2.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailureKind {
    Uncaught = 0,
    NotConstant = 1,
    StepBudget = 2,
    MemoryBudget = 3,
    DepthLimit = 4,
    HostFailure = 5,
}

/// A place in the transform's source (§4.6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourcePlace {
    pub file: Vec<u8>,
    pub line: u64,
    pub column: u64,
}

/// The `slip::error` an uncaught exception carried (§4.6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HostRefusal {
    pub function: String,
    /// The enumerator's name (`spec/api.md` §4).
    pub errc: String,
    pub message: String,
    pub subject: Option<Place>,
}

/// The record of a failed run (§4.6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Failure {
    pub kind: FailureKind,
    /// The diagnostic's first line.
    pub message: String,
    /// Where in the transform's source it arose.
    pub place: Option<SourcePlace>,
    /// The diagnostics as Kiln renders them, the call stack included.
    pub rendered: String,
    /// For `Uncaught` with a `slip::error`.
    pub refusal: Option<HostRefusal>,
}

/// The `edits` section (§4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edits {
    /// [`SCHEME`].
    pub scheme: u64,
    /// The request id (§3).
    pub transform: Digest,
    /// The run failed (§4.6): every list but `files` is then empty.
    pub failure: Option<Failure>,
    /// The files this section names; sorted by path, no two equal.
    pub files: Vec<FileRef>,
    /// Sorted by `id`, no two equal.
    pub subjects: Vec<Subject>,
    /// Sorted by `(a, b)`, no two equal.
    pub links: Vec<Link>,
    /// Sorted by [`Claim::cmp_key`], no two equal in it.
    pub claims: Vec<Claim>,
    /// Indices of `Replace` claims applied all or none (§4.4).
    pub groups: Vec<Vec<u64>>,
    /// Sorted by `(place, severity, message, attached)`, no two equal.
    pub reports: Vec<Report>,
    /// Sorted by `(key, place, reason)`, no two equal.
    pub changed: Vec<Changed>,
    /// A rule table (`spec/verify.md` §2): sorted by the bytes of the
    /// encoded `(parent, kind, name, disc)`, no two with the same.
    pub renamed: Vec<Rule>,
}

impl Piece {
    /// The tag of §4.2.
    pub fn tag(&self) -> u64 {
        match self {
            Piece::Literal(_) => 0,
            Piece::Copy(_) => 1,
            Piece::Spelled { .. } => 2,
        }
    }

    /// The order of §4.2: by tag, then by fields; bytes and ids by their bytes.
    pub fn cmp_key(&self, other: &Piece) -> Ordering {
        match (self, other) {
            (Piece::Literal(a), Piece::Literal(b)) => a.cmp(b),
            (Piece::Copy(a), Piece::Copy(b)) => a.cmp(b),
            (Piece::Spelled { key: ka, text: ta }, Piece::Spelled { key: kb, text: tb }) => {
                ka.as_bytes().cmp(kb.as_bytes()).then_with(|| ta.cmp(tb))
            }
            _ => self.tag().cmp(&other.tag()),
        }
    }
}

impl Claim {
    /// The sort key of §4.2, `(file, lo, hi, stance, pieces)`, as an
    /// `Ordering`: `pieces` as a sequence, a proper prefix first.
    pub fn cmp_key(&self, other: &Claim) -> Ordering {
        (self.file, self.lo, self.hi, self.stance)
            .cmp(&(other.file, other.lo, other.hi, other.stance))
            .then_with(|| {
                for (a, b) in self.pieces.iter().zip(&other.pieces) {
                    match a.cmp_key(b) {
                        Ordering::Equal => {}
                        o => return o,
                    }
                }
                self.pieces.len().cmp(&other.pieces.len())
            })
    }

    /// §4.3: another derivation equal in the sort key joined into this one.
    fn join(&mut self, other: Claim) {
        self.votes = self.votes.saturating_add(other.votes);
        self.tiers |= other.tiers;
        self.macro_site = match (self.macro_site.take(), other.macro_site) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.reason = match (self.reason, other.reason) {
            (Reason::None, r) | (r, Reason::None) => r,
            (a, b) => a.min(b),
        };
        self.subjects.extend(other.subjects);
        sort_unique(&mut self.subjects);
        self.notes.extend(other.notes);
        sort_unique(&mut self.notes);
    }
}

fn sort_unique<T: Ord>(v: &mut Vec<T>) {
    v.sort();
    v.dedup();
}

/// The `(parent, kind, name, disc)` bytes rules are ordered by.
fn rule_order(a: &Rule, b: &Rule) -> Ordering {
    a.match_bytes().cmp(&b.match_bytes()).then_with(|| a.cmp(b))
}

impl Edits {
    /// What a producer calls before it hands the section over. Sorts
    /// every list in its order; joins claims equal in the sort key as
    /// §4.3 says (votes summed, tiers or-ed, the least `MacroSite`, the
    /// least reason other than `none`, `subjects` and `notes` united);
    /// carries `groups` through the new indices, joins groups that share
    /// a member and drops groups of one (§4.4); removes equal reports,
    /// `changed` entries and rules. The result satisfies §4.7 if the
    /// records did one by one.
    ///
    /// Also: `files` is sorted by path and equal entries are made one,
    /// every file index (claims, copy pieces, reports, `changed`, the
    /// refusal's subject) following them; equal subjects and links are
    /// made one, each link with `a < b`, each `of` sorted. What normalising
    /// cannot repair is left for [`crate::Shard::validate`] to name: two
    /// files with one path and different contents, two different subjects
    /// with one id, two rules with one match and different targets.
    pub fn normalise(&mut self) {
        // Files, and every index into them.
        let mut order: Vec<usize> = (0..self.files.len()).collect();
        order.sort_by(|&a, &b| {
            let (fa, fb) = (&self.files[a], &self.files[b]);
            (&fa.path, fa.size, fa.content).cmp(&(&fb.path, fb.size, fb.content))
        });
        let mut new_index = vec![0u64; self.files.len()];
        let mut files: Vec<FileRef> = Vec::with_capacity(self.files.len());
        for old in order {
            if files.last() != Some(&self.files[old]) {
                files.push(self.files[old].clone());
            }
            new_index[old] = (files.len() - 1) as u64;
        }
        self.files = files;
        let remap = |file: &mut u64| {
            if let Some(&n) = usize::try_from(*file).ok().and_then(|i| new_index.get(i)) {
                *file = n;
            }
        };
        for c in &mut self.claims {
            remap(&mut c.file);
            for p in &mut c.pieces {
                if let Piece::Copy(place) = p {
                    remap(&mut place.file);
                }
            }
        }
        for r in &mut self.reports {
            if let Some(place) = &mut r.place {
                remap(&mut place.file);
            }
        }
        for c in &mut self.changed {
            remap(&mut c.place.file);
        }
        if let Some(Some(place)) =
            self.failure.as_mut().map(|f| f.refusal.as_mut().and_then(|r| r.subject.as_mut()))
        {
            remap(&mut place.file);
        }

        // Subjects and links.
        for s in &mut self.subjects {
            sort_unique(&mut s.of);
        }
        self.subjects.sort_by(|a, b| {
            a.id.cmp(&b.id).then_with(|| {
                (a.role, &a.of, a.flags, &a.display, &a.rule)
                    .cmp(&(b.role, &b.of, b.flags, &b.display, &b.rule))
            })
        });
        self.subjects.dedup();
        for l in &mut self.links {
            if l.a > l.b {
                std::mem::swap(&mut l.a, &mut l.b);
            }
        }
        sort_unique(&mut self.links);

        // Claims (§4.2, §4.3), remembering where each one went.
        let claims = std::mem::take(&mut self.claims);
        let mut order: Vec<(usize, Claim)> = claims.into_iter().enumerate().collect();
        for (_, c) in &mut order {
            sort_unique(&mut c.subjects);
            sort_unique(&mut c.notes);
        }
        order.sort_by(|a, b| a.1.cmp_key(&b.1).then(a.0.cmp(&b.0)));
        let mut moved = vec![0u64; order.len()];
        let mut joined: Vec<Claim> = Vec::with_capacity(order.len());
        for (old, c) in order {
            match joined.last_mut() {
                Some(last) if last.cmp_key(&c) == Ordering::Equal => last.join(c),
                _ => joined.push(c),
            }
            moved[old] = (joined.len() - 1) as u64;
        }
        self.claims = joined;

        // Groups (§4.4): through the new indices, joined where they share
        // a member, groups of one dropped.
        let groups = std::mem::take(&mut self.groups);
        let mut parent: BTreeMap<u64, u64> = BTreeMap::new();
        fn root(parent: &mut BTreeMap<u64, u64>, x: u64) -> u64 {
            let mut r = x;
            while let Some(&p) = parent.get(&r) {
                if p == r {
                    break;
                }
                r = p;
            }
            parent.insert(x, r);
            r
        }
        for g in &groups {
            let members: Vec<u64> = g
                .iter()
                .map(|&m| {
                    usize::try_from(m).ok().and_then(|i| moved.get(i)).copied().unwrap_or(m)
                })
                .collect();
            for &m in &members {
                parent.entry(m).or_insert(m);
            }
            if let Some((&first, rest)) = members.split_first() {
                for &m in rest {
                    let (a, b) = (root(&mut parent, first), root(&mut parent, m));
                    if a != b {
                        parent.insert(a.max(b), a.min(b));
                    }
                }
            }
        }
        let mut sets: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        let members: Vec<u64> = parent.keys().copied().collect();
        for m in members {
            let r = root(&mut parent, m);
            sets.entry(r).or_default().push(m);
        }
        let mut groups: Vec<Vec<u64>> = sets.into_values().filter(|g| g.len() >= 2).collect();
        groups.sort();
        self.groups = groups;

        // Reports, declarations, rules.
        sort_unique(&mut self.reports);
        sort_unique(&mut self.changed);
        self.renamed.sort_by(rule_order);
        self.renamed.dedup();
    }
}
