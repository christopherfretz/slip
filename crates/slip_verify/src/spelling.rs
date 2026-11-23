//! The spelled entities over the run (`spec/verify.md` §10.3, §10.4 and
//! the run's half of §10.6): the fold of the `spellings` sections of this
//! run's `ok` shards into one outcome per obligation of the overlay. Work
//! package WP-070.
//!
//! Pure and part of the trusted base. The fold keeps one slot per
//! obligation and no shard, so its memory does not grow with the number
//! of observers. [`Fold::add`] checks a section against the overlay and the
//! shard's `inputs` before it changes anything: a section it refuses
//! leaves the fold as it was, and the caller treats the shard as not this
//! run's (§10.4).
//!
//! An obligation is never made to pass by what the fold does not know: it
//! is `Passed` only when some section said it holds and none that it
//! fails, and `Unseen` (a failure, §10.3) when no section said either.

use slip_model::spellings::Spellings;
use slip_model::{Digest, FileRef, Overlay};

/// What the run says of one obligation (§10.3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// It holds in at least one observer and fails in none; `not_seen`
    /// counts the observers that read its file and neither held nor failed
    /// it.
    Passed { not_seen: u64 },
    /// It fails in `observers` observers; `first` is the plan position (the
    /// `position` given to [`Fold::add`]) of the first of them, and `found`
    /// and `name` are what that observer found (§10.4).
    DenotesOther { observers: u64, first: usize, found: Option<Digest>, name: String },
    /// No observer held or failed it.
    Unseen,
}

impl Outcome {
    /// Whether the obligation passed (§10.3): only `Passed` does.
    pub fn passed(&self) -> bool {
        matches!(self, Outcome::Passed { .. })
    }
}

/// The first failure of an obligation and the number of observers it
/// failed in.
#[derive(Clone, Debug)]
struct Failure {
    observers: u64,
    first: usize,
    found: Option<Digest>,
    name: String,
}

/// One obligation's slot.
#[derive(Clone, Debug, Default)]
struct Slot {
    held: bool,
    not_seen: u64,
    failed: Option<Failure>,
}

/// The fold of §10.3: one slot per obligation of the overlay.
#[derive(Clone, Debug)]
pub struct Fold {
    /// The overlay's file paths, in its order (sorted, no two equal).
    paths: Vec<Vec<u8>>,
    /// Per obligation, the index of its file.
    file_of: Vec<u64>,
    slots: Vec<Slot>,
}

/// What one obligation is in the section being added.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    NotSeen,
    Held,
    Failed(usize),
}

impl Fold {
    /// A fold over the obligations of `overlay` (its `spellings`, in order).
    ///
    /// The overlay is trusted to satisfy its invariants (`spec/edits.md`
    /// §7.1: files sorted, obligations sorted by file, each file an index):
    /// it is read from the store by `Overlay::from_bytes`, which checks
    /// them. Over an overlay that breaks them the fold stays total and an
    /// obligation it cannot place is `Unseen`.
    pub fn new(overlay: &Overlay) -> Fold {
        Fold {
            paths: overlay.files.iter().map(|f| f.path.clone()).collect(),
            file_of: overlay.spellings.iter().map(|o| o.file).collect(),
            slots: vec![Slot::default(); overlay.spellings.len()],
        }
    }

    /// The number of obligations.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the overlay has no obligation.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// One `ok` shard of this run, of the observer at plan position
    /// `position`, with its `inputs` and its `spellings` section.
    ///
    /// `Err`, and the fold unchanged, if the section breaks an invariant
    /// of §10.4 (spans ascending, not touching, of at least one index;
    /// failures strictly ascending; no index held and failed; no more
    /// indices than `seen`); if it names an obligation the overlay does not
    /// have; if it names one whose file `inputs` does not list; or if its
    /// `seen` is not the number of obligations whose file `inputs` lists.
    pub fn add(
        &mut self,
        position: usize,
        inputs: &[FileRef],
        section: &Spellings,
    ) -> Result<(), String> {
        self.add_first(position, inputs, section).map(|_| ())
    }

    /// [`Fold::add`], and whether `position` is now the `first` of some
    /// obligation's failure.
    pub(crate) fn add_first(
        &mut self,
        position: usize,
        inputs: &[FileRef],
        section: &Spellings,
    ) -> Result<bool, String> {
        let marks = self.marks(inputs, section)?;
        let mut first = false;
        for (i, mark) in marks {
            let slot = &mut self.slots[i];
            match mark {
                Mark::NotSeen => slot.not_seen = slot.not_seen.saturating_add(1),
                Mark::Held => slot.held = true,
                Mark::Failed(f) => {
                    let f = &section.failed[f];
                    match &mut slot.failed {
                        None => {
                            slot.failed = Some(Failure {
                                observers: 1,
                                first: position,
                                found: f.found,
                                name: f.name.clone(),
                            });
                            first = true;
                        }
                        Some(g) => {
                            g.observers = g.observers.saturating_add(1);
                            // The first in plan order, whatever the order
                            // of the calls.
                            if position < g.first {
                                g.first = position;
                                g.found = f.found;
                                g.name.clone_from(&f.name);
                                first = true;
                            }
                        }
                    }
                }
            }
        }
        Ok(first)
    }

    /// Whether [`Fold::add`] would accept the section, changing nothing:
    /// the check of a shard whose spellings are not folded (§10.6).
    pub(crate) fn check(&self, inputs: &[FileRef], section: &Spellings) -> Result<(), String> {
        self.marks(inputs, section).map(|_| ())
    }

    /// Every obligation whose file `inputs` lists, ascending, with what the
    /// section says of it; or the reason the section is refused. Changes
    /// nothing.
    fn marks(&self, inputs: &[FileRef], s: &Spellings) -> Result<Vec<(usize, Mark)>, String> {
        let n = self.slots.len() as u64;
        // The invariants of §10.4, again: the fold is total over any value,
        // not only over a validated shard.
        let mut end: Option<u64> = None;
        for span in &s.held {
            if span.count == 0 {
                return Err("spellings: a held span is empty".to_owned());
            }
            if end.is_some_and(|e| span.first <= e) {
                return Err("spellings: held spans are not ascending and apart".to_owned());
            }
            let Some(last) = span.first.checked_add(span.count) else {
                return Err("spellings: a held span overflows".to_owned());
            };
            if last > n {
                return Err(format!(
                    "spellings: obligation {} is not one of the overlay's {n}",
                    span.first.max(n)
                ));
            }
            end = Some(last);
        }
        for (j, f) in s.failed.iter().enumerate() {
            if j > 0 && s.failed[j - 1].index >= f.index {
                return Err("spellings: failed obligations are not strictly ascending".to_owned());
            }
            if f.index >= n {
                return Err(format!(
                    "spellings: obligation {} is not one of the overlay's {n}",
                    f.index
                ));
            }
            if held(&s.held, f.index) {
                return Err(format!("spellings: obligation {} is held and failed", f.index));
            }
        }
        // That no more indices are held or failed than `seen` follows from
        // the checks below: they are distinct, each is of a file read, and
        // `seen` must be the number of those files' obligations.
        // The overlay's files the shard read, ascending by index.
        let mut read: Vec<usize> = inputs
            .iter()
            .filter_map(|f| self.paths.binary_search_by(|p| p.as_slice().cmp(&f.path)).ok())
            .collect();
        read.sort_unstable();
        read.dedup();
        let is_read = |i: u64| {
            let file = self.file_of[i as usize];
            usize::try_from(file).is_ok_and(|f| read.binary_search(&f).is_ok())
        };
        // Every index the section names is of a file the shard read.
        for span in &s.held {
            if let Some(i) = (span.first..span.first + span.count).find(|&i| !is_read(i)) {
                return Err(self.unread(i));
            }
        }
        if let Some(f) = s.failed.iter().find(|f| !is_read(f.index)) {
            return Err(self.unread(f.index));
        }
        // The obligations of those files, which are contiguous per file
        // and ascending with the file (the overlay's order).
        let mut marks = Vec::new();
        let (mut h, mut f) = (0, 0);
        for &file in &read {
            let lo = self.file_of.partition_point(|&x| x < file as u64);
            let hi = self.file_of.partition_point(|&x| x <= file as u64);
            for i in lo..hi {
                let ix = i as u64;
                while h < s.held.len() && s.held[h].first + s.held[h].count <= ix {
                    h += 1;
                }
                while f < s.failed.len() && s.failed[f].index < ix {
                    f += 1;
                }
                let mark = if f < s.failed.len() && s.failed[f].index == ix {
                    Mark::Failed(f)
                } else if h < s.held.len() && s.held[h].first <= ix {
                    Mark::Held
                } else {
                    Mark::NotSeen
                };
                marks.push((i, mark));
            }
        }
        if marks.len() as u64 != s.seen {
            return Err(format!(
                "spellings: seen is {}, but the files the shard read have {} obligations",
                s.seen,
                marks.len()
            ));
        }
        Ok(marks)
    }

    fn unread(&self, i: u64) -> String {
        format!(
            "spellings: obligation {i} is of {}, which the shard did not read",
            String::from_utf8_lossy(self.path_of(i as usize))
        )
    }

    /// The outcome of every obligation so far, in the overlay's order.
    pub(crate) fn outcomes(&self) -> Vec<Outcome> {
        self.slots
            .iter()
            .map(|s| match &s.failed {
                Some(f) => Outcome::DenotesOther {
                    observers: f.observers,
                    first: f.first,
                    found: f.found,
                    name: f.name.clone(),
                },
                None if s.held => Outcome::Passed { not_seen: s.not_seen },
                None => Outcome::Unseen,
            })
            .collect()
    }

    /// The outcome of every obligation, in the overlay's order.
    pub fn finish(self) -> Vec<Outcome> {
        self.outcomes()
    }

    /// The overlay's file path of obligation `i`; empty if the overlay
    /// was not valid (an obligation's file not one of its files).
    pub(crate) fn path_of(&self, i: usize) -> &[u8] {
        self.file_of
            .get(i)
            .and_then(|&f| usize::try_from(f).ok())
            .and_then(|f| self.paths.get(f))
            .map_or(&[], Vec::as_slice)
    }
}

/// Whether `index` is in one of `spans` (ascending, apart).
fn held(spans: &[slip_model::spellings::Span], index: u64) -> bool {
    let k = spans.partition_point(|s| s.first <= index);
    k > 0 && index - spans[k - 1].first < spans[k - 1].count
}
