//! Groups (`spec/edits.md` §5.5): the ties of every shard, joined by a
//! union-find over the identities of the grouped edits. The effect is
//! [`crate::geometry::effect`].

use std::collections::BTreeMap;

use slip_model::Digest;

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
