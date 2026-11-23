//! The records of agreement between observers (`spec/verify.md` §9.1):
//! sections 24, `binding`, and 25, `rule_use`, of a verify shard. Work
//! package WP-060.
//!
//! Plain data. The binary and JSON forms and the invariants are in
//! `formats` (`spec/landed/edits/WP-060.md`).

use crate::Digest;

/// How one rule was used by a walk (`spec/verify.md` §9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Use {
    /// The rule's index in `Declared.rules`.
    pub rule: u64,
    /// Entities the rule gave their key.
    pub via: u64,
    /// Entities whose mapped key is the key the rule produces, and whose
    /// own key no rule was applied to.
    pub direct: u64,
}

/// Section 25, `rule_use` (`spec/verify.md` §9.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleUse {
    /// The `rules_id` of the table the walk had.
    pub rules: Digest,
    /// Sorted by `rule`, no two equal; only rules with `via` or `direct`
    /// not 0.
    pub uses: Vec<Use>,
}

/// Section 24, `binding` (`spec/verify.md` §9.1): what a verify shard was
/// made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    /// Content id of the `OVLY` container, for an image of the overlay
    /// form; absent for a tree.
    pub overlay: Option<Digest>,
    /// Content id of the `DECL` container the job was given.
    pub declared: Digest,
    /// The round of the pre shard (`spec/verify.md` §5.2 step 0).
    pub pre: u64,
}
