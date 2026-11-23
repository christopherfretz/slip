//! The text of a range, expansions and obligations (`spec/edits.md` §7.1
//! "What the edits mean", step 5).
//!
//! For one post-image: the edits it needs (its outermost edits and,
//! through every copy, the edits each copy carries), the files they read,
//! then each expansion in the layout's order, each held until its last use.
//! An expansion carries the ranges, in its own bytes, where spelled text
//! landed, so a landing carried by a copy is shifted with it and every
//! landing in a post-image is one obligation.

use std::collections::BTreeMap;

use slip_model::edits::{KeyId, Piece};

use crate::ApplyError;
use crate::Files;
use crate::build::{Layout, files_needed, read_checked};

/// Spelled text that landed: `[lo, hi)` in the bytes it is part of, and its key.
pub(crate) type Landing = (u64, u64, KeyId);

/// An expansion, or the text of a range: its bytes and its landings, ascending.
#[derive(Debug, Default)]
pub(crate) struct Expansion {
    pub bytes: Vec<u8>,
    pub spells: Vec<Landing>,
}

impl Expansion {
    fn append(&mut self, other: &Expansion) {
        let shift = self.bytes.len() as u64;
        self.spells.extend(other.spells.iter().map(|&(lo, hi, k)| (lo + shift, hi + shift, k)));
        self.bytes.extend_from_slice(&other.bytes);
    }
}

/// The state kept across the post-images of one build: per-edit marks,
/// use counts and expansions, reset by epoch, so one post-image costs what
/// its own edits cost.
#[derive(Debug)]
pub(crate) struct Work<'a, 'b> {
    lay: &'b Layout<'a>,
    position: Vec<usize>,
    mark: Vec<u32>,
    epoch: u32,
    uses: Vec<usize>,
    expansions: Vec<Option<Expansion>>,
}

impl<'a, 'b> Work<'a, 'b> {
    pub fn new(lay: &'b Layout<'a>) -> Work<'a, 'b> {
        let n = lay.edits();
        let mut position = vec![0; n];
        for (k, &e) in lay.order.iter().enumerate() {
            position[e] = k;
        }
        Work {
            lay,
            position,
            mark: vec![0; n],
            epoch: 0,
            uses: vec![0; n],
            expansions: (0..n).map(|_| None).collect(),
        }
    }

    /// The post-image of file `f` and its landings.
    pub fn post_image(
        &mut self,
        f: usize,
        files: &dyn Files,
    ) -> Result<(Vec<u8>, Vec<Landing>), ApplyError> {
        let lay = self.lay;
        self.epoch += 1;
        let epoch = self.epoch;
        // The edits this post-image needs, and how often each is used.
        let mut needed = Vec::new();
        let mut stack = Vec::new();
        for &r in &lay.children[lay.root(f)] {
            self.mark[r] = epoch;
            self.uses[r] = 1;
            needed.push(r);
            stack.push(r);
        }
        while let Some(e) = stack.pop() {
            for c in lay.copies_of(e) {
                for &m in lay.carried(c) {
                    if self.mark[m] != epoch {
                        self.mark[m] = epoch;
                        self.uses[m] = 0;
                        needed.push(m);
                        stack.push(m);
                    }
                    self.uses[m] += 1;
                }
            }
        }
        // The files they read, in path order.
        let mut bytes: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        for g in files_needed(lay, f, &needed) {
            bytes.insert(g, read_checked(lay.set, g, files)?);
        }
        needed.sort_unstable_by_key(|&e| self.position[e]);
        for &e in &needed {
            let x = self.expand(e, &bytes);
            debug_assert_eq!(
                x.bytes.len() as u64,
                lay.lengths[e],
                "an expansion has its planned length"
            );
            self.expansions[e] = Some(x);
        }
        let size = lay.set.files[f].size;
        let roots = &lay.children[lay.root(f)];
        let mut post = Expansion::default();
        self.text(&mut post, &bytes[&f], roots, 0, size);
        debug_assert!(
            needed.iter().all(|&e| self.expansions[e].is_none()),
            "every expansion was used"
        );
        Ok((post.bytes, post.spells))
    }

    /// The expansion of edit `e`: its pieces in order.
    fn expand(&mut self, e: usize, bytes: &BTreeMap<usize, Vec<u8>>) -> Expansion {
        let lay = self.lay;
        let mut out = Expansion::default();
        let mut copy = lay.copy_at[e];
        for p in &lay.edit(e).pieces {
            match p {
                Piece::Literal(b) => out.bytes.extend_from_slice(b),
                Piece::Spelled { key, text } => {
                    let lo = out.bytes.len() as u64;
                    out.bytes.extend_from_slice(text);
                    out.spells.push((lo, out.bytes.len() as u64, *key));
                }
                Piece::Copy(_) => {
                    let c = lay.copies[copy];
                    copy += 1;
                    self.text(&mut out, &bytes[&c.file], lay.carried(&c), c.lo, c.hi);
                }
            }
        }
        out
    }

    /// Appends the text of `[lo, hi)` of `file` to `out`: its bytes with
    /// each edit of `carried` (the edits properly inside the range and
    /// within no other edit properly inside it, ascending) replaced by its
    /// expansion. Each expansion used is released after its last use.
    fn text(&mut self, out: &mut Expansion, file: &[u8], carried: &[usize], lo: u64, hi: u64) {
        let lay = self.lay;
        let mut at = lo as usize;
        for &m in carried {
            let edit = lay.edit(m);
            out.bytes.extend_from_slice(&file[at..edit.lo as usize]);
            let x = self.expansions[m]
                .as_ref()
                .unwrap_or_else(|| unreachable!("edit {m} is expanded before it is used"));
            out.append(x);
            self.uses[m] -= 1;
            if self.uses[m] == 0 {
                self.expansions[m] = None;
            }
            at = edit.hi as usize;
        }
        out.bytes.extend_from_slice(&file[at..hi as usize]);
    }
}
