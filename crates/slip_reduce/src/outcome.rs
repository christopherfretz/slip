//! The outcome of one site (`spec/edits.md` §5.4's first table), from its
//! collapsed claims after §5.2's treatment of unselected subjects.

use std::collections::BTreeSet;

use slip_model::Digest;
use slip_model::edits::{MacroSite, Reason, Stance};
use slip_model::editset::Cause;

use crate::records::{ClaimVal, KPiece, Sample, least_macro, union};

/// What the table makes of a site.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// A candidate edit.
    Live { pieces: Vec<KPiece>, val: ClaimVal },
    Kept,
    Unresolved,
    /// Rejected by the table or by `mixed_set` (§5.2).
    Rejected { cause: Cause, reason: Reason, macro_site: Option<MacroSite>, sample: Sample },
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

fn join_sample(acc: &mut Option<Sample>, s: &Sample) {
    match acc {
        Some(a) => a.join(s),
        None => *acc = Some(s.clone()),
    }
}

/// The table, with §5.2's treatment first. When several rows reject, the
/// cause is the first of `mixed_set`, `site_refused`, `divergent`,
/// `contested` (landed WP-062 §4).
pub(crate) fn site(claims: Vec<(Stance, Vec<KPiece>, ClaimVal)>, selected: &BTreeSet<Digest>) -> SiteOut {
    let mut keep: Option<Sample> = None;
    let mut unresolved: Option<Sample> = None;
    let mut refuse: Option<ClaimVal> = None;
    let mut replaces: Vec<(Vec<KPiece>, ClaimVal)> = Vec::new();
    let mut mixed: Vec<ClaimVal> = Vec::new();
    let mut subjects: Vec<Digest> = Vec::new();
    let mut edits = Vec::new();
    for (stance, pieces, val) in claims {
        let n = val.subjects.len();
        let s = val.subjects.iter().filter(|k| selected.contains(k)).count();
        let none_selected = n > 0 && s == 0;
        let some_not = n > 0 && s > 0 && s < n;
        match stance {
            Stance::Replace if none_selected => join_sample(&mut keep, &val.sample),
            Stance::Replace if some_not => {
                edits.push((pieces, val.subjects.clone()));
                mixed.push(val.clone());
            }
            Stance::Replace => {
                edits.push((pieces.clone(), val.subjects.clone()));
                replaces.push((pieces, val.clone()));
            }
            Stance::Refuse if none_selected => continue,
            Stance::Refuse if some_not => mixed.push(val.clone()),
            Stance::Refuse => match &mut refuse {
                Some(r) => r.join(&val),
                None => refuse = Some(val.clone()),
            },
            Stance::Keep => join_sample(&mut keep, &val.sample),
            Stance::Unresolved => join_sample(&mut unresolved, &val.sample),
        }
        subjects = union(&subjects, &val.subjects);
    }
    let least_subject = subjects.first().copied();

    let outcome = if !mixed.is_empty() {
        let mut sample = Sample::default();
        let mut m = None;
        for v in &mixed {
            sample.join(&v.sample);
            m = least_macro(m, v.macro_site.clone());
        }
        Outcome::Rejected { cause: Cause::MixedSet, reason: Reason::None, macro_site: m, sample }
    } else if let Some(r) = refuse {
        Outcome::Rejected {
            cause: Cause::SiteRefused,
            reason: r.reason,
            macro_site: r.macro_site,
            sample: r.sample,
        }
    } else if replaces.len() >= 2 {
        let mut sample = Sample::default();
        let mut m = None;
        for (_, v) in &replaces {
            sample.join(&v.sample);
            m = least_macro(m, v.macro_site.clone());
        }
        Outcome::Rejected { cause: Cause::Divergent, reason: Reason::None, macro_site: m, sample }
    } else if let Some((pieces, val)) = replaces.pop() {
        match &keep {
            Some(k) => Outcome::Rejected {
                cause: Cause::Contested,
                reason: Reason::None,
                macro_site: val.macro_site,
                sample: k.clone(),
            },
            None => Outcome::Live { pieces, val },
        }
    } else if keep.is_some() {
        Outcome::Kept
    } else if unresolved.is_some() {
        Outcome::Unresolved
    } else {
        Outcome::Nothing
    };

    let blocking = match outcome {
        Outcome::Live { .. } => unresolved,
        _ => match (keep, unresolved) {
            (Some(mut k), Some(u)) => {
                k.join(&u);
                Some(k)
            }
            (k, u) => k.or(u),
        },
    };
    SiteOut { outcome, blocking, least_subject, edits }
}
