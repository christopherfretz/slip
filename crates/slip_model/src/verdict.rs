//! The `verdict` section of a verify shard (`spec/verify.md` §6): what the
//! comparison of one observer's pre- and post-image digests found.
//!
//! Plain data. The binary and JSON forms are implemented in `formats`.

use crate::Digest;

/// The outcome of comparing one observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerdictKind {
    Verified = 0,
    Mismatch = 1,
    Unverifiable = 2,
}

/// What a finding says about a unit. The derived order is that of the
/// kinds' numbers in the format, not the order of findings: findings are
/// ordered by [`FindingKind::rank`] (`spec/verify.md` §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FindingKind {
    /// In both images with different digests.
    Changed = 0,
    /// In the pre-image only.
    Removed = 1,
    /// In the post-image only.
    Added = 2,
    /// Flagged indeterminate in either image.
    Indeterminate = 3,
    /// A derived unit both images have, with different digests.
    DerivedChanged = 4,
    /// A derived unit both images have, indeterminate in either.
    DerivedIndeterminate = 5,
    /// A derived unit marked emitted unreferenced in the pre-image only
    /// (`spec/verify.md` §4 step 3; WP-155).
    DerivedOnlyPre = 6,
    /// A derived unit marked emitted unreferenced in the post-image only.
    DerivedOnlyPost = 7,
}

impl FindingKind {
    /// The place of the kind in the order of findings (`spec/verify.md` §6):
    /// `changed` 0, `removed` 1, `added` 2, `derived_changed` 3,
    /// `derived_only_pre` 4, `derived_only_post` 5, `indeterminate` 6,
    /// `derived_indeterminate` 7. Not the kind's number in the format.
    pub fn rank(self) -> u8 {
        match self {
            FindingKind::Changed => 0,
            FindingKind::Removed => 1,
            FindingKind::Added => 2,
            FindingKind::DerivedChanged => 3,
            FindingKind::DerivedOnlyPre => 4,
            FindingKind::DerivedOnlyPost => 5,
            FindingKind::Indeterminate => 6,
            FindingKind::DerivedIndeterminate => 7,
        }
    }
}

/// A place in one image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Site {
    /// Subject path; empty if unknown.
    pub file: Vec<u8>,
    /// 1-based; 0 with `column` 0 if the node has no location.
    pub line: u64,
    pub column: u64,
    /// The node's kind, by name.
    pub node: String,
    /// The differing field, rendered.
    pub what: String,
}

/// Where two streams first differ (`spec/verify.md` §6.1). For people; no
/// verdict depends on it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Where {
    pub pre: Site,
    pub post: Site,
}

/// One difference between the two images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    /// Absent only for an `Indeterminate` finding that could not be named.
    pub key: Option<Digest>,
    /// The group's file (the pre-image's group if there is one); empty if unknown.
    pub file: Vec<u8>,
    /// The unit's digest in each image.
    pub pre: Option<Digest>,
    pub post: Option<Digest>,
    /// The entity as a person would write it; for the report only.
    pub name: String,
    pub location: Option<Where>,
}

/// The `verdict` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub kind: VerdictKind,
    /// Content id of the pre shard compared against; all zero if there was none.
    pub pre: Digest,
    /// `rules_id` of the declared changes the post-image was digested with.
    pub rules: Digest,
    /// For `Unverifiable`; else empty.
    pub reason: String,
    /// Derived keys only one image has.
    pub only_pre: u64,
    pub only_post: u64,
    /// Exemptions that prevented a finding; sorted, no two equal.
    pub used: Vec<Digest>,
    /// The number of findings, of which `findings` holds the first ones.
    pub total: u64,
    /// Ordered by `(rank, file, key)`, the rank of [`FindingKind::rank`].
    pub findings: Vec<Finding>,
}
