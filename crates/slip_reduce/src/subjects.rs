//! Subjects (`spec/edits.md` §5.2): their union over the shards, the
//! selection, and the refusals of subjects.

use std::collections::{BTreeMap, BTreeSet};

use slip_model::Digest;
use slip_model::edits::{Role, Subject, subject_flags};
use slip_model::editset::Cause;
use slip_model::keys::Rule;

use crate::records::{Sample, union};

/// The join of every shard's record of one subject (§5.2 rule 1).
#[derive(Clone, Debug)]
pub(crate) struct SubjectJoin {
    /// The rule of the least observer index that has the subject.
    pub rule: Option<Rule>,
    pub rule_from: u64,
    /// The least observer index whose record's rule differs from that of
    /// the least observer index (an `Input` error, §5.2 rule 1).
    pub conflict: Option<u64>,
    /// `seed` if any record is, else `candidate` if any is, else `follower`.
    pub role: Role,
    /// The union of the `of` of every record (only a follower's is not empty).
    pub of: Vec<Digest>,
    pub flags: u64,
    pub display_from: u64,
    pub display: String,
    /// The observers that have the subject.
    pub seen: Sample,
    /// The observers whose record sets each flag bit.
    pub flag_seen: [Sample; 4],
}

fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

impl SubjectJoin {
    pub fn of_record(s: &Subject, index: u64) -> SubjectJoin {
        let mut flag_seen: [Sample; 4] = Default::default();
        for (bit, seen) in flag_seen.iter_mut().enumerate() {
            if s.flags & (1 << bit) != 0 {
                *seen = Sample::one(index);
            }
        }
        SubjectJoin {
            rule: s.rule.clone(),
            rule_from: index,
            conflict: None,
            role: s.role,
            of: s.of.clone(),
            flags: s.flags,
            display_from: index,
            display: s.display.clone(),
            seen: Sample::one(index),
            flag_seen,
        }
    }

    /// The join; associative and commutative, so the result does not
    /// depend on the order in which shards are read.
    pub fn join(&mut self, o: &SubjectJoin) {
        let mut conflict = min_opt(self.conflict, o.conflict);
        if self.rule != o.rule {
            conflict = min_opt(conflict, Some(self.rule_from.max(o.rule_from)));
        }
        self.conflict = conflict;
        if o.rule_from < self.rule_from {
            self.rule.clone_from(&o.rule);
            self.rule_from = o.rule_from;
        }
        self.role = self.role.min(o.role);
        self.of = union(&self.of, &o.of);
        self.flags |= o.flags;
        if o.display_from < self.display_from {
            self.display_from = o.display_from;
            self.display.clone_from(&o.display);
        }
        self.seen.join(&o.seen);
        for (a, b) in self.flag_seen.iter_mut().zip(&o.flag_seen) {
            a.join(b);
        }
    }

    /// The record of the edit set (§6): `of` only for a follower.
    pub fn to_subject(&self, id: Digest) -> Subject {
        Subject {
            id,
            rule: self.rule.clone(),
            role: self.role,
            of: if self.role == Role::Follower { self.of.clone() } else { Vec::new() },
            flags: self.flags,
            display: self.display.clone(),
        }
    }
}

/// A refusal of a subject (§5.2 rule 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubjectRefusal {
    pub cause: Cause,
    pub subject: Option<Digest>,
    pub sample: Sample,
}

/// The selection of §5.2 rule 2 and the refusals of rule 3.
#[derive(Debug)]
pub(crate) struct Selection {
    pub selected: BTreeSet<Digest>,
    pub refusals: Vec<SubjectRefusal>,
}

/// The flags and the cause each refuses with, in the order of the table.
const FLAG_CAUSES: [(u64, usize, Cause); 4] = [
    (subject_flags::READ_ONLY, 0, Cause::SubjectReadOnly),
    (subject_flags::UNSUPPORTED, 1, Cause::SubjectUnsupported),
    (subject_flags::UNREACHABLE, 3, Cause::SubjectUnreachable),
    (subject_flags::NAME_TAKEN, 2, Cause::NameTaken),
];

/// Selects (§5.2 rule 2): the least set holding every seed, closed under
/// links in both directions, and holding a follower once every entity in
/// its `of` is selected.
pub(crate) fn select(
    subjects: &BTreeMap<Digest, SubjectJoin>,
    links: &BTreeSet<(Digest, Digest)>,
    rename: bool,
) -> Selection {
    let mut adjacent: BTreeMap<Digest, Vec<Digest>> = BTreeMap::new();
    for &(a, b) in links {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    let mut selected: BTreeSet<Digest> = BTreeSet::new();
    let mut queue: Vec<Digest> = Vec::new();
    for (id, s) in subjects {
        if s.role == Role::Seed {
            selected.insert(*id);
            queue.push(*id);
        }
    }
    loop {
        while let Some(x) = queue.pop() {
            for y in adjacent.get(&x).into_iter().flatten() {
                if selected.insert(*y) {
                    queue.push(*y);
                }
            }
        }
        for (id, s) in subjects {
            if s.role == Role::Follower
                && !selected.contains(id)
                && !s.of.is_empty()
                && s.of.iter().all(|k| selected.contains(k))
            {
                selected.insert(*id);
                queue.push(*id);
            }
        }
        if queue.is_empty() {
            break;
        }
    }

    let mut refusals = Vec::new();
    if rename && !subjects.values().any(|s| s.role == Role::Seed) {
        refusals.push(SubjectRefusal {
            cause: Cause::NoSubject,
            subject: None,
            sample: Sample::default(),
        });
    }
    for id in &selected {
        // A selected id that is a link's end is a subject of that shard,
        // so it is in `subjects` (an `Input` error otherwise).
        let Some(s) = subjects.get(id) else { continue };
        for (flag, bit, cause) in FLAG_CAUSES {
            if s.flags & flag != 0 {
                refusals.push(SubjectRefusal {
                    cause,
                    subject: Some(*id),
                    sample: s.flag_seen[bit].clone(),
                });
            }
        }
    }
    // `mixed_follower`: an entity some observer saw as a follower whose
    // selection does not follow its `of` (landed WP-062 §3).
    for (id, s) in subjects {
        if s.of.is_empty() {
            continue;
        }
        let n = s.of.iter().filter(|k| selected.contains(k)).count();
        let all = n == s.of.len();
        if (n > 0 && !all) || selected.contains(id) != all {
            refusals.push(SubjectRefusal {
                cause: Cause::MixedFollower,
                subject: Some(*id),
                sample: s.seen.clone(),
            });
        }
    }
    Selection { selected, refusals }
}
