//! The outcome of one site (`spec/edits.md` §5.4's first table), from its
//! collapsed claims after §5.2's treatment of unselected subjects.

use std::collections::BTreeSet;

use slip_model::Digest;
use slip_model::edits::{MacroSite, Reason, Stance};
use slip_model::editset::Cause;

use crate::records::{ClaimVal, KPiece, Sample, Site, least_macro, union, variant};

/// What the table makes of a site.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// A candidate edit.
    Live {
        pieces: Vec<KPiece>,
        val: ClaimVal,
    },
    Kept,
    Unresolved,
    /// Rejected by the table or by `mixed_set` (§5.2).
    Rejected {
        cause: Cause,
        reason: Reason,
        macro_site: Option<MacroSite>,
        sample: Sample,
    },
    /// Every claim was dropped (§5.2: a `refuse` all of whose subjects are
    /// unselected).
    Nothing,
}

/// A site's outcome and what the rest of reduce needs of it.
#[derive(Debug)]
pub(crate) struct SiteOut {
    pub outcome: Outcome,
    /// The site's `keep` and `unresolved` claims, if it has any and is not
    /// live, or its `unresolved` claims if it is live: a range that
    /// contests every other live site it shares a byte with (§5.4).
    pub blocking: Option<Sample>,
    /// The least subject of the site's claims that remain.
    pub least_subject: Option<Digest>,
    /// Every `replace` claim that remains a `replace` (a `mixed_set` one
    /// included): the edits of the site, by pieces, with their subjects.
    pub edits: Vec<(Vec<KPiece>, Vec<Digest>)>,
}

/// The observers of a set of claims of one site, each counted once: those
/// that made one of them alone, and those whose several claims at the site
/// include one of them (landed WP-062 §4).
fn exact(
    claims: &[(Digest, Stance, ClaimVal)],
    observed: &[(Vec<Digest>, Sample)],
    class: &BTreeSet<Digest>,
) -> Sample {
    let mut out = Sample::default();
    for (v, _, val) in claims {
        if class.contains(v) {
            out.join(&val.single);
        }
    }
    for (variants, s) in observed {
        if variants.iter().any(|v| class.contains(v)) {
            out.join(s);
        }
    }
    out
}

/// The table, with §5.2's treatment first. When several rows reject, the
/// cause is the first of `mixed_set`, `site_refused`, `divergent`,
/// `contested` (landed WP-062 §4). `observed`: the site's
/// [`Entry::Observed`](crate::records::Entry::Observed) records.
#[allow(clippy::too_many_lines)]
pub(crate) fn site(
    at: &Site,
    claims: Vec<(Stance, Vec<KPiece>, ClaimVal)>,
    observed: &[(Vec<Digest>, Sample)],
    selected: &BTreeSet<Digest>,
) -> SiteOut {
    let claims: Vec<(Digest, Stance, Vec<KPiece>, ClaimVal)> =
        claims.into_iter().map(|(st, p, v)| (variant(at, st, &p), st, p, v)).collect();
    let mut keep: BTreeSet<Digest> = BTreeSet::new();
    let mut unresolved: BTreeSet<Digest> = BTreeSet::new();
    let mut refuse: Option<ClaimVal> = None;
    let mut refuse_class: BTreeSet<Digest> = BTreeSet::new();
    let mut replaces: Vec<(Vec<KPiece>, ClaimVal)> = Vec::new();
    let mut replace_class: BTreeSet<Digest> = BTreeSet::new();
    let mut mixed: Vec<ClaimVal> = Vec::new();
    let mut mixed_class: BTreeSet<Digest> = BTreeSet::new();
    let mut subjects: Vec<Digest> = Vec::new();
    let mut edits = Vec::new();
    let mut vals: Vec<(Digest, Stance, ClaimVal)> = Vec::with_capacity(claims.len());
    for (v, stance, pieces, val) in claims {
        let n = val.subjects.len();
        let s = val.subjects.iter().filter(|k| selected.contains(k)).count();
        let none_selected = n > 0 && s == 0;
        let some_not = n > 0 && s > 0 && s < n;
        match stance {
            Stance::Replace if none_selected => {
                keep.insert(v);
            }
            Stance::Replace if some_not => {
                edits.push((pieces, val.subjects.clone()));
                mixed.push(val.clone());
                mixed_class.insert(v);
            }
            Stance::Replace => {
                edits.push((pieces.clone(), val.subjects.clone()));
                replaces.push((pieces, val.clone()));
                replace_class.insert(v);
            }
            Stance::Refuse if none_selected => continue,
            Stance::Refuse if some_not => {
                mixed.push(val.clone());
                mixed_class.insert(v);
            }
            Stance::Refuse => {
                refuse_class.insert(v);
                match &mut refuse {
                    Some(r) => r.join(&val),
                    None => refuse = Some(val.clone()),
                }
            }
            Stance::Keep => {
                keep.insert(v);
            }
            Stance::Unresolved => {
                unresolved.insert(v);
            }
        }
        subjects = union(&subjects, &val.subjects);
        vals.push((v, stance, val));
    }
    let least_subject = subjects.first().copied();
    let count = |class: &BTreeSet<Digest>| exact(&vals, observed, class);

    let outcome = if !mixed.is_empty() {
        let mut m = None;
        for v in &mixed {
            m = least_macro(m, v.macro_site.clone());
        }
        Outcome::Rejected {
            cause: Cause::MixedSet,
            reason: Reason::None,
            macro_site: m,
            sample: count(&mixed_class),
        }
    } else if let Some(r) = refuse {
        Outcome::Rejected {
            cause: Cause::SiteRefused,
            reason: r.reason,
            macro_site: r.macro_site,
            sample: count(&refuse_class),
        }
    } else if replaces.len() >= 2 {
        let mut m = None;
        for (_, v) in &replaces {
            m = least_macro(m, v.macro_site.clone());
        }
        Outcome::Rejected {
            cause: Cause::Divergent,
            reason: Reason::None,
            macro_site: m,
            sample: count(&replace_class),
        }
    } else if let Some((pieces, val)) = replaces.pop() {
        if keep.is_empty() {
            Outcome::Live { pieces, val }
        } else {
            Outcome::Rejected {
                cause: Cause::Contested,
                reason: Reason::None,
                macro_site: val.macro_site,
                sample: count(&keep),
            }
        }
    } else if !keep.is_empty() {
        Outcome::Kept
    } else if !unresolved.is_empty() {
        Outcome::Unresolved
    } else {
        Outcome::Nothing
    };

    let blocking = if let Outcome::Live { .. } = outcome {
        (!unresolved.is_empty()).then(|| count(&unresolved))
    } else {
        let both: BTreeSet<Digest> = keep.union(&unresolved).copied().collect();
        (!both.is_empty()).then(|| count(&both))
    };
    SiteOut { outcome, blocking, least_subject, edits }
}
