//! `splice` (`spec/edits.md` §7.1, last step for one file): disjoint
//! replacements of one buffer.

use crate::ApplyError;

/// Replaces each `(lo, hi)` of `pre` by its text. The edits are ascending
/// and disjoint: `lo <= hi <= pre.len()`, each `lo` at or after the `hi` of
/// the one before, and no two equal `(lo, hi)` (so two insertions at one
/// offset have no order and are refused). `Invalid` otherwise. An empty
/// list returns `pre`.
pub fn splice(pre: &[u8], edits: &[(u64, u64, &[u8])]) -> Result<Vec<u8>, ApplyError> {
    let len = pre.len() as u64;
    let mut size = len;
    let mut prev: Option<(u64, u64)> = None;
    for (i, &(lo, hi, text)) in edits.iter().enumerate() {
        if lo > hi || hi > len {
            return Err(ApplyError::Invalid(format!(
                "splice: edit {i} [{lo}, {hi}) is not a range of the {len} bytes"
            )));
        }
        if let Some((plo, phi)) = prev
            && (lo < phi || (plo, phi) >= (lo, hi))
        {
            return Err(ApplyError::Invalid(format!(
                "splice: edit {i} [{lo}, {hi}) is not after edit {} [{plo}, {phi})",
                i - 1
            )));
        }
        prev = Some((lo, hi));
        size = (size - (hi - lo)).saturating_add(text.len() as u64);
    }
    let mut out = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    let mut at = 0usize;
    for &(lo, hi, text) in edits {
        out.extend_from_slice(&pre[at..lo as usize]);
        out.extend_from_slice(text);
        at = hi as usize;
    }
    out.extend_from_slice(&pre[at..]);
    Ok(out)
}
