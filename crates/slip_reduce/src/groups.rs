//! Groups (`spec/edits.md` §5.5): the ties of every shard, joined by a
//! union-find over the identities of the grouped edits, and the effect of
//! the edits that remain.

use std::collections::{BTreeMap, BTreeSet};

use slip_model::Digest;

use crate::geometry::{Span, parents, properly_inside, properly_inside_of};

/// A union-find over edit identities: one entry per distinct grouped edit
/// (docs/wp/WP-062.md "Group ties in pass 1").
#[derive(Clone, Debug, Default)]
pub(crate) struct Ties {
    parent: BTreeMap<Digest, Digest>,
}

impl Ties {
    /// The root of `x`'s class, if `x` is a grouped edit.
    pub fn find(&mut self, x: &Digest) -> Option<Digest> {
        let mut r = *self.parent.get(x)?;
        let mut path = Vec::new();
        while let Some(&p) = self.parent.get(&r) {
            if p == r {
                break;
            }
            path.push(r);
            r = p;
        }
        for q in path {
            self.parent.insert(q, r);
        }
        self.parent.insert(*x, r);
        Some(r)
    }

    fn add(&mut self, x: Digest) -> Digest {
        if let Some(r) = self.find(&x) {
            r
        } else {
            self.parent.insert(x, x);
            x
        }
    }

    /// Ties `a` and `b`. The least digest is the root, so that the
    /// structure does not depend on the order of the unions.
    pub fn union(&mut self, a: Digest, b: Digest) {
        let (ra, rb) = (self.add(a), self.add(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent.insert(hi, lo);
        }
    }

    /// Ties the members of one shard's group.
    pub fn tie(&mut self, members: &[Digest]) {
        if let Some((first, rest)) = members.split_first() {
            self.add(*first);
            for m in rest {
                self.union(*first, *m);
            }
        }
    }

    /// The join with another's ties.
    pub fn join(&mut self, mut other: Ties) {
        let keys: Vec<Digest> = other.parent.keys().copied().collect();
        for x in keys {
            if let Some(r) = other.find(&x) {
                self.union(x, r);
            }
        }
    }
}

/// One round of §5.5's effect over the edits that remain (`spans`, sorted
/// by `(lo, hi)`, no two crossing and none crossing a copied range;
/// `copies` the copied ranges of each, by index): every edit that is not
/// effective, with the innermost remaining edit that contains it.
///
/// An edit is outermost if it is within no other; effective if outermost
/// or carried by an effective edit's copy in which it is maximal (no other
/// remaining edit properly inside the copied range contains it). Only the
/// edits that are within another are looked at; a run of a native rename
/// has none and returns at once.
pub(crate) fn not_effective(spans: &[Span], copies: &[(usize, Span)]) -> Vec<(usize, usize)> {
    let parent = parents(spans);
    if parent.is_empty() {
        return Vec::new();
    }
    let mut by_owner: BTreeMap<usize, Vec<Span>> = BTreeMap::new();
    for &(e, r) in copies {
        by_owner.entry(e).or_default().push(r);
    }
    let mut effective: BTreeSet<usize> = BTreeSet::new();
    let mut queue: Vec<usize> = by_owner.keys().copied().filter(|e| !parent.contains_key(e)).collect();
    while let Some(e) = queue.pop() {
        for &r in by_owner.get(&e).into_iter().flatten() {
            for (l, h) in properly_inside_of(spans, r) {
                for x in l..h {
                    let Some(&p) = parent.get(&x) else { continue };
                    if properly_inside(spans[p], r) || effective.contains(&x) {
                        continue;
                    }
                    effective.insert(x);
                    if by_owner.contains_key(&x) {
                        queue.push(x);
                    }
                }
            }
        }
    }
    parent.iter().filter(|(x, _)| !effective.contains(x)).map(|(&x, &p)| (x, p)).collect()
}
