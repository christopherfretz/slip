//! The mutated edit sets of the mutation oracle (`spec/harness.md` §6.4):
//! the run's edit set with one edit dropped, with one edit added, or with
//! the key of one spelled piece complemented. Each is a valid `EDTS`
//! (`spec/edits.md` §6.1) whenever the original is: what a mutation implies
//! for the rest of the set (group indices, `in_force`, the files no record
//! names any more, the state of a set left with no edit) is carried
//! through, and nothing else changes (`spec/landed/harness/WP-077.md` §3).

use slip_model::Piece;
use slip_model::edits::{Place, Reason};
use slip_model::editset::{Cause, Edit, EditSet, Refusal, SetState, cross, share_byte};

/// The most edits dropped, added, and keys changed (§6.4).
const SAMPLE: usize = 8;
const AT_MOST: usize = 4;

/// What a mutation did, for judging its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Edit `index` was dropped: its file's real path and its range. The
    /// set may then be refused (it was the only edit).
    Dropped { index: usize, file: Vec<u8>, lo: u64, hi: u64 },
    /// An edit was added at `kept[index]`.
    Added { kept: usize },
    /// The key of spelled piece `piece` of edit `edit` was complemented.
    Key { edit: usize, piece: usize },
}

/// One mutated edit set.
#[derive(Debug, Clone)]
pub struct Mutation {
    pub label: String,
    pub kind: Kind,
    pub set: EditSet,
}

/// The mutated edit sets of spec/harness.md §6.4, in the order given there.
pub fn mutations(set: &EditSet) -> Vec<(String, EditSet)> {
    all(set).into_iter().map(|m| (m.label, m.set)).collect()
}

/// [`mutations`] with what each did.
pub fn all(set: &EditSet) -> Vec<Mutation> {
    let mut out = Vec::new();
    for i in dropped_indices(set.edits.len()) {
        let e = &set.edits[i];
        let file = usize::try_from(e.file)
            .ok()
            .and_then(|f| set.files.get(f))
            .map(|f| f.path.clone())
            .unwrap_or_default();
        out.push(Mutation {
            label: format!("drop edit {i}"),
            kind: Kind::Dropped { index: i, file, lo: e.lo, hi: e.hi },
            set: drop_edit(set, i),
        });
    }
    if let Some(first) = set.edits.first() {
        let mut added = 0;
        for (k, place) in set.kept.iter().enumerate() {
            if added == AT_MOST {
                break;
            }
            if !can_add_at(set, place) {
                continue;
            }
            added += 1;
            out.push(Mutation {
                label: format!("add edit at kept {k}"),
                kind: Kind::Added { kept: k },
                set: add_edit(set, first, place),
            });
        }
    }
    let mut keys = 0;
    'edits: for (i, e) in set.edits.iter().enumerate() {
        for (j, p) in e.pieces.iter().enumerate() {
            if keys == AT_MOST {
                break 'edits;
            }
            if let Piece::Spelled { .. } | Piece::Identifier { key: Some(_), .. } = p {
                keys += 1;
                out.push(Mutation {
                    label: format!("change key of edit {i} piece {j}"),
                    kind: Kind::Key { edit: i, piece: j },
                    set: complement_key(set, i, j),
                });
            }
        }
    }
    out
}

/// Every index if there are at most 8 edits, else `⌊k·n/8⌋` for `k = 0..7`.
pub fn dropped_indices(n: usize) -> Vec<usize> {
    if n <= SAMPLE { (0..n).collect() } else { (0..SAMPLE).map(|k| k * n / SAMPLE).collect() }
}

fn drop_edit(set: &EditSet, i: usize) -> EditSet {
    let mut s = set.clone();
    s.edits.remove(i);
    let i = i as u64;
    let mut groups: Vec<Vec<u64>> = s
        .groups
        .iter()
        .map(|g| g.iter().filter(|&&m| m != i).map(|&m| if m > i { m - 1 } else { m }).collect())
        .filter(|g: &Vec<u64>| g.len() >= 2)
        .collect();
    groups.sort();
    s.groups = groups;
    if s.edits.is_empty() {
        // An accepted set has an edit (§6.1): with none left it is refused,
        // and a refused set has a refusal; `no_edit` says no edit is left.
        s.state = SetState::Refused;
        s.groups.clear();
        if s.refusals.is_empty() {
            s.refusals.push(Refusal {
                cause: Cause::NoEdit,
                file: None,
                lo: 0,
                hi: 0,
                reason: Reason::None,
                subject: None,
                macro_site: None,
                other: None,
                observers: 0,
                first: Vec::new(),
            });
        }
    }
    recompute_in_force(&mut s);
    prune_files(&mut s);
    s
}

/// Whether an edit with the first edit's pieces can stand at `place`: the range
/// is not empty, no edit has it, it crosses no edit and no range an edit
/// copies, and no range it copies crosses an edit (§6.1).
fn can_add_at(set: &EditSet, place: &Place) -> bool {
    let range = (place.lo, place.hi);
    if place.lo >= place.hi {
        return false;
    }
    for e in &set.edits {
        if e.file == place.file && ((e.lo, e.hi) == range || cross((e.lo, e.hi), range)) {
            return false;
        }
        for p in &e.pieces {
            if let Piece::Copy(c) = p
                && c.file == place.file
                && cross((c.lo, c.hi), range)
            {
                return false;
            }
        }
    }
    // The new edit's own copies are the first edit's, which already cross
    // no edit of the set; the loop above checked them against the range.
    true
}

fn add_edit(set: &EditSet, first: &Edit, place: &Place) -> EditSet {
    let mut s = set.clone();
    let new = Edit {
        file: place.file,
        lo: place.lo,
        hi: place.hi,
        pieces: first.pieces.clone(),
        subjects: first.subjects.clone(),
        tiers: first.tiers,
        // The invocation is of the first edit's place, not this one.
        macro_site: None,
        notes: first.notes.clone(),
        observers: first.observers,
        votes: first.votes,
        first: first.first.clone(),
    };
    let at = s.edits.partition_point(|e| (e.file, e.lo, e.hi) < (new.file, new.lo, new.hi));
    s.edits.insert(at, new);
    let at = at as u64;
    for g in &mut s.groups {
        for m in g.iter_mut() {
            if *m >= at {
                *m += 1;
            }
        }
    }
    recompute_in_force(&mut s);
    s
}

fn complement_key(set: &EditSet, i: usize, j: usize) -> EditSet {
    let mut s = set.clone();
    if let Piece::Spelled { key, .. } | Piece::Identifier { key: Some(key), .. } =
        &mut s.edits[i].pieces[j]
    {
        for b in &mut key.0 {
            *b = !*b;
        }
    }
    s
}

/// `in_force` of every `changed` entry: an edit shares a byte with its
/// place (§6.2).
fn recompute_in_force(s: &mut EditSet) {
    for c in &mut s.changed {
        c.in_force = s
            .edits
            .iter()
            .any(|e| e.file == c.place.file && share_byte((e.lo, e.hi), (c.place.lo, c.place.hi)));
    }
}

/// Removes the files no record names (§6.1 row 12, the records of
/// `spec/landed/edits/WP-061.md` §2) and renumbers every index into
/// `files`.
fn prune_files(s: &mut EditSet) {
    let mut named = vec![false; s.files.len()];
    {
        let mut mark = |f: u64| {
            if let Some(n) = usize::try_from(f).ok().and_then(|f| named.get_mut(f)) {
                *n = true;
            }
        };
        for e in &s.edits {
            mark(e.file);
            for p in &e.pieces {
                if let Piece::Copy(c) = p {
                    mark(c.file);
                }
            }
        }
        for c in &s.changed {
            mark(c.place.file);
        }
        for p in s.kept.iter().chain(&s.unresolved) {
            mark(p.file);
        }
        for r in &s.refusals {
            if let Some(f) = r.file {
                mark(f);
            }
            if let Some(o) = &r.other {
                mark(o.file);
            }
        }
        for r in &s.reports {
            if let Some(p) = &r.report.place {
                mark(p.file);
            }
        }
        for f in &s.failures {
            if let Some(p) = f.failure.refusal.as_ref().and_then(|r| r.subject.as_ref()) {
                mark(p.file);
            }
        }
    }
    if named.iter().all(|n| *n) {
        return;
    }
    let mut index = vec![0u64; named.len()];
    let mut next = 0u64;
    for (i, n) in named.iter().enumerate() {
        index[i] = next;
        if *n {
            next += 1;
        }
    }
    let files = std::mem::take(&mut s.files);
    s.files = files.into_iter().zip(&named).filter(|(_, n)| **n).map(|(f, _)| f).collect();
    let re = |f: &mut u64| {
        if let Some(n) = usize::try_from(*f).ok().and_then(|i| index.get(i)) {
            *f = *n;
        }
    };
    for e in &mut s.edits {
        re(&mut e.file);
        for p in &mut e.pieces {
            if let Piece::Copy(c) = p {
                re(&mut c.file);
            }
        }
    }
    for c in &mut s.changed {
        re(&mut c.place.file);
    }
    for p in s.kept.iter_mut().chain(s.unresolved.iter_mut()) {
        re(&mut p.file);
    }
    for r in &mut s.refusals {
        if let Some(f) = &mut r.file {
            re(f);
        }
        if let Some(o) = &mut r.other {
            re(&mut o.file);
        }
    }
    for r in &mut s.reports {
        if let Some(p) = &mut r.report.place {
            re(&mut p.file);
        }
    }
    for f in &mut s.failures {
        if let Some(p) = f.failure.refusal.as_mut().and_then(|r| r.subject.as_mut()) {
            re(&mut p.file);
        }
    }
}
