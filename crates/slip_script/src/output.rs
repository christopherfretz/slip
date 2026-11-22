//! The run output (`spec/api.md` §7.6) and its canonical form.
//!
//! The host records requests as they come into a raw [`RunOutput`] (edits
//! in request order, groups as lists of raw edit indices) and
//! [`RunOutput::canonical`] turns it into the form §7.6 fixes: nothing in
//! it depends on the order of the requests or on a number the host
//! assigned. The encoding in a shard is `spec/edits.md`'s, not this
//! module's.

use slip_model::keys::Rule;
use slip_model::{Counter, Digest, FileRef};

/// A run of bytes of a file, named by the file's identity (§7.6
/// `(file, lo, hi)`). Ordered by file, then `lo`, then `hi`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    pub file: FileRef,
    pub lo: u32,
    pub hi: u32,
}

/// One piece of a replacement (§7.1, §7.6 `Piece`). The derived order
/// (literal, then copy, then spelled; then by content) is the order of
/// edits with the same range.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Piece {
    Literal(Vec<u8>),
    Copy(Place),
    /// The entity meant (its key) and the text chosen for it (§7.2).
    Spelled {
        key: Digest,
        text: Vec<u8>,
    },
}

/// One requested replacement (§7.6 `Edit`). Its identity is its place and
/// its pieces; the notes and the tier are not part of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edit {
    /// `lo < hi`.
    pub place: Place,
    pub pieces: Vec<Piece>,
    /// 0 or 1 (§5.4).
    pub tier: u8,
    /// Sorted, distinct (§7.3 rule 5).
    pub notes: Vec<Vec<u8>>,
}

/// `slip::severity` (§7.4). Open in the header; the host knows these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Severity {
    Note = 0,
    Review = 1,
    Error = 2,
}

impl Severity {
    /// The severity of a wire value; `None` for one the host does not know.
    pub fn from_wire(v: u64) -> Option<Severity> {
        match v {
            0 => Some(Severity::Note),
            1 => Some(Severity::Review),
            2 => Some(Severity::Error),
            _ => None,
        }
    }
}

/// One report (§7.4, §7.6 `Report`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Report {
    pub place: Option<Place>,
    pub severity: Severity,
    pub message: Vec<u8>,
    /// §7.4 rule 3: `true` when the report is attached to the nearest
    /// place the host can name instead of the place of the value it was
    /// given (`spec/landed/api/WP-081.md`).
    pub attached: bool,
}

/// One declared change (§7.5, §7.6 `Changed`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Changed {
    pub key: Digest,
    pub place: Place,
    pub reason: Vec<u8>,
}

/// The output of a completed run (§7.6 `RunOutput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOutput {
    pub transform: Digest,
    pub edits: Vec<Edit>,
    /// Canonical: each a sorted list of two or more indices into `edits`,
    /// no edit in two groups, the list sorted. Raw (while the host
    /// records): one list per group made, of raw edit indices, in any
    /// order and with any repetition.
    pub groups: Vec<Vec<usize>>,
    pub reports: Vec<Report>,
    pub changed: Vec<Changed>,
    /// `spec/verify.md` §2.1; sorted as a rule table.
    pub renamed: Vec<Rule>,
    pub counters: Vec<Counter>,
}

impl RunOutput {
    /// An output with nothing in it.
    pub fn empty(transform: Digest) -> RunOutput {
        RunOutput {
            transform,
            edits: Vec::new(),
            groups: Vec::new(),
            reports: Vec::new(),
            changed: Vec::new(),
            renamed: Vec::new(),
            counters: Vec::new(),
        }
    }

    /// The canonical form of §7.6: pieces normalised (adjacent literals
    /// joined, empty literals dropped, §7.1 rule 6); edits sorted by
    /// (place, pieces) and identical ones joined (notes unioned, the
    /// higher tier kept); groups rewritten to the joined indices, groups
    /// that share a member merged, groups of fewer than two members
    /// dropped; reports, declared changes and rules sorted and
    /// deduplicated; counters sorted by name.
    ///
    /// `self.groups` is read as raw: indices into `self.edits` as given.
    /// The function is total: an index out of range names no edit and is
    /// ignored.
    #[must_use]
    pub fn canonical(self) -> RunOutput {
        let RunOutput { transform, edits, groups, mut reports, mut changed, renamed, mut counters } =
            self;

        // Edits: normalise, sort, join.
        let raw_len = edits.len();
        let mut order: Vec<(usize, Edit)> = edits
            .into_iter()
            .enumerate()
            .map(|(i, mut e)| {
                e.pieces = normalise(std::mem::take(&mut e.pieces));
                (i, e)
            })
            .collect();
        order.sort_by(|a, b| (&a.1.place, &a.1.pieces).cmp(&(&b.1.place, &b.1.pieces)));
        let mut joined: Vec<Edit> = Vec::with_capacity(order.len());
        let mut new_index = vec![0usize; raw_len];
        for (raw, edit) in order {
            match joined.last_mut() {
                Some(last) if last.place == edit.place && last.pieces == edit.pieces => {
                    last.notes.extend(edit.notes);
                    last.tier = last.tier.max(edit.tier);
                }
                _ => joined.push(edit),
            }
            new_index[raw] = joined.len() - 1;
        }
        for e in &mut joined {
            e.notes.sort();
            e.notes.dedup();
        }

        // Groups: union-find over the joined edits.
        let mut parent: Vec<usize> = (0..joined.len()).collect();
        let mut grouped = vec![false; joined.len()];
        for group in &groups {
            let mut first: Option<usize> = None;
            for &raw in group {
                let Some(&e) = new_index.get(raw) else { continue };
                grouped[e] = true;
                match first {
                    None => first = Some(e),
                    Some(f) => union(&mut parent, f, e),
                }
            }
        }
        let mut by_root: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (e, _) in grouped.iter().enumerate().filter(|(_, g)| **g) {
            let r = find(&mut parent, e);
            by_root.entry(r).or_default().push(e);
        }
        let mut groups: Vec<Vec<usize>> = by_root.into_values().filter(|g| g.len() >= 2).collect();
        groups.sort();

        reports.sort();
        reports.dedup();
        changed.sort();
        changed.dedup();

        let mut renamed: Vec<(Vec<u8>, Rule)> = renamed
            .into_iter()
            .map(|r| {
                let mut enc = slip_model::wire::Encoder::new();
                r.encode(&mut enc);
                (enc.into_bytes(), r)
            })
            .collect();
        // The encoding of a rule begins with the encoding of its match,
        // which is self-delimiting, so ordering by the whole encoding is
        // ordering by the match first: the rule table's order.
        renamed.sort_by(|a, b| a.0.cmp(&b.0));
        renamed.dedup_by(|a, b| a.0 == b.0);
        let renamed = renamed.into_iter().map(|(_, r)| r).collect();

        counters.sort();
        counters.dedup();

        RunOutput { transform, edits: joined, groups, reports, changed, renamed, counters }
    }

    /// Whether the output is already in canonical form.
    pub fn is_canonical(&self) -> bool {
        self.clone().canonical() == *self
    }
}

/// §7.1 rule 6: adjacent literals are one piece; an empty literal is no piece.
fn normalise(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for p in pieces {
        match p {
            Piece::Literal(bytes) if bytes.is_empty() => {}
            Piece::Literal(bytes) => {
                if let Some(Piece::Literal(prev)) = out.last_mut() {
                    prev.extend_from_slice(&bytes);
                } else {
                    out.push(Piece::Literal(bytes));
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi] = lo;
    }
}
