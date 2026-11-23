//! Claims before §4.3's join: one derivation per name token the parser
//! saw, located by `slip_kiln::sites` (`spec/edits.md` §2.2, §2.3, §4.2).
//!
//! A derivation is one span Kiln gave, with one stance. Two written
//! entries of one span (a name the table records in two roles) are one
//! derivation: their subjects are united and, for a `replace`, two
//! different spelled keys make it a literal. Derivations of different
//! spans that land on one range of a file (an argument substituted
//! twice, a header included twice) stay apart and become claims of one
//! vote each, which `Edits::normalise` joins (§4.3). The piece of a
//! `replace` is decided over all derivations of its range ("The
//! pieces", §9.3): all spelled with one key, `spelled(K, new)`;
//! anything else, `literal(new)`.
//!
//! Nothing here computes a range: every range is a `Located` answer of
//! `Sites::name_token` for a span Kiln gave, or `[0, 0)` of the main
//! file where `sites` gives none (§4.2).

use std::collections::{BTreeMap, BTreeSet};

use slip_kiln::kiln::source::{FileId, Span};
use slip_kiln::sites::{Located, Sites};
use slip_model::FileRef;
use slip_model::edits::{Claim, KeyId, MacroSite, Piece, Reason, Stance};

/// How a replacement is written (§9.3 "The pieces").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Written {
    /// `literal(new)`: a declaration's name, or a token whose
    /// derivations have different origins.
    Literal,
    /// `spelled(K, new)`, `K` the key id of the referent's origin.
    Spelled(KeyId),
}

impl Written {
    /// Two derivations of one token: one key stays spelled, anything else
    /// is a literal.
    fn join(self, other: Written) -> Written {
        if self == other { self } else { Written::Literal }
    }
}

/// One derivation.
#[derive(Debug)]
struct Derivation {
    /// The range; `None` is `[0, 0)` of the main file.
    at: Option<(FileId, u64, u64)>,
    stance: Stance,
    written: Option<Written>,
    subjects: BTreeSet<KeyId>,
    reason: Reason,
    tier: u64,
    macro_site: Option<MacroSite>,
}

/// The derivations of a run, by the span Kiln gave and the stance.
#[derive(Debug)]
pub(crate) struct Claims {
    old: Vec<u8>,
    new: Vec<u8>,
    derivations: BTreeMap<(u32, u32, Stance), Derivation>,
}

impl Claims {
    /// No derivation yet; `old` is the spelling `sites` checks a name
    /// token against, `new` the text of every piece.
    pub fn new(old: &[u8], new: &[u8]) -> Claims {
        Claims { old: old.to_vec(), new: new.to_vec(), derivations: BTreeMap::new() }
    }

    /// The name token `name` denotes `subjects` and becomes the new name,
    /// written as `written` says; a `refuse` with `sites`' reason when it
    /// cannot be edited.
    pub fn replace(&mut self, sites: &Sites<'_, '_>, name: Span, subjects: &[KeyId], written: Written) {
        match sites.name_token(name, &self.old) {
            Located::At { file, lo, hi, tier, macro_site } => self.add(
                name,
                Derivation {
                    at: Some((file, lo, hi)),
                    stance: Stance::Replace,
                    written: Some(written),
                    subjects: subjects.iter().copied().collect(),
                    reason: Reason::None,
                    tier,
                    macro_site,
                },
            ),
            Located::Refused { reason, tier, at, macro_site } => self.add(
                name,
                Derivation {
                    at,
                    stance: Stance::Refuse,
                    written: None,
                    subjects: subjects.iter().copied().collect(),
                    reason: reason.wire(),
                    tier,
                    macro_site,
                },
            ),
        }
    }

    /// The name token `name` denotes something that is not a subject.
    /// Dropped when it cannot be located (nothing would be edited there).
    pub fn keep(&mut self, sites: &Sites<'_, '_>, name: Span) {
        self.located_only(sites, name, Stance::Keep);
    }

    /// The name token `name` may denote a subject; this observer cannot
    /// tell. Dropped when it cannot be located.
    pub fn unresolved(&mut self, sites: &Sites<'_, '_>, name: Span) {
        self.located_only(sites, name, Stance::Unresolved);
    }

    /// The name token `name` would have to change and cannot, for
    /// `reason` (one of §4.2's that `sites` does not give: `mixed_set`,
    /// `mixed_instances`). Its range is the token's when `sites` locates
    /// it, else the range of `sites`' own refusal.
    #[allow(dead_code, reason = "for docs/wp/WP-073.md (mixed_set, mixed_instances)")]
    pub fn refuse(&mut self, sites: &Sites<'_, '_>, name: Span, subjects: &[KeyId], reason: Reason) {
        let (at, tier, macro_site) = match sites.name_token(name, &self.old) {
            Located::At { file, lo, hi, tier, macro_site } => (Some((file, lo, hi)), tier, macro_site),
            Located::Refused { tier, at, macro_site, .. } => (at, tier, macro_site),
        };
        self.add(
            name,
            Derivation {
                at,
                stance: Stance::Refuse,
                written: None,
                subjects: subjects.iter().copied().collect(),
                reason,
                tier,
                macro_site,
            },
        );
    }

    fn located_only(&mut self, sites: &Sites<'_, '_>, name: Span, stance: Stance) {
        if let Located::At { file, lo, hi, tier, macro_site } = sites.name_token(name, &self.old) {
            self.add(
                name,
                Derivation {
                    at: Some((file, lo, hi)),
                    stance,
                    written: None,
                    subjects: BTreeSet::new(),
                    reason: Reason::None,
                    tier,
                    macro_site,
                },
            );
        }
    }

    /// Records `d` for span `name`, or joins it to the derivation of that
    /// span with that stance.
    fn add(&mut self, name: Span, d: Derivation) {
        let key = (name.lo.to_raw(), name.hi.to_raw(), d.stance);
        match self.derivations.get_mut(&key) {
            None => {
                self.derivations.insert(key, d);
            }
            Some(have) => {
                have.subjects.extend(d.subjects);
                have.written = match (have.written, d.written) {
                    (Some(a), Some(b)) => Some(a.join(b)),
                    (a, b) => a.or(b),
                };
                have.reason = match (have.reason, d.reason) {
                    (Reason::None, r) | (r, Reason::None) => r,
                    (a, b) => a.min(b),
                };
            }
        }
    }

    /// The claims, one vote each, and the files they name (in `FileId`
    /// order; `Edits::normalise` sorts them by path). `files` gives the
    /// `FileRef` of a buffer; `main` is the main file, the file of a
    /// refusal `sites` gives no range for.
    ///
    /// # Errors
    ///
    /// A claim in a buffer `files` does not know.
    pub fn finish(
        self,
        files: &dyn Fn(FileId) -> Option<FileRef>,
        main: FileId,
    ) -> Result<(Vec<FileRef>, Vec<Claim>), String> {
        // The piece of each replaced range, over all its derivations.
        let mut pieces: BTreeMap<(FileId, u64, u64), Written> = BTreeMap::new();
        for d in self.derivations.values() {
            if let (Stance::Replace, Some(at), Some(w)) = (d.stance, d.at, d.written) {
                pieces.entry(at).and_modify(|have| *have = have.join(w)).or_insert(w);
            }
        }
        let mut index: BTreeMap<FileId, u64> = BTreeMap::new();
        let mut refs: Vec<FileRef> = Vec::new();
        let mut claims = Vec::with_capacity(self.derivations.len());
        for d in self.derivations.into_values() {
            let (file, lo, hi) = d.at.unwrap_or((main, 0, 0));
            let i = match index.get(&file) {
                Some(&i) => i,
                None => {
                    let r = files(file).ok_or_else(|| {
                        format!("a claim lies in buffer {} that is not one of the inputs", file.index())
                    })?;
                    refs.push(r);
                    let i = (refs.len() - 1) as u64;
                    index.insert(file, i);
                    i
                }
            };
            let pieces = match (d.stance, pieces.get(&(file, lo, hi))) {
                (Stance::Replace, Some(Written::Spelled(key))) => {
                    vec![Piece::Spelled { key: *key, text: self.new.clone() }]
                }
                (Stance::Replace, _) => vec![Piece::Literal(self.new.clone())],
                _ => Vec::new(),
            };
            claims.push(Claim {
                file: i,
                lo,
                hi,
                stance: d.stance,
                pieces,
                subjects: d.subjects.into_iter().collect(),
                reason: d.reason,
                tiers: d.tier,
                macro_site: d.macro_site,
                votes: 1,
                notes: Vec::new(),
            });
        }
        Ok((refs, claims))
    }
}
