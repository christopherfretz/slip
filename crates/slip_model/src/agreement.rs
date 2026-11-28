//! The records of agreement between observers (`spec/verify.md` §9.1):
//! sections 24, `binding`, 29, `rule_use`, 33, `rebuilt_use` (WP-118),
//! and 34, `existing_use` (WP-144), of a verify shard. Work package WP-060.
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
    /// For a rule not of kind `namespace`: entities whose mapped key is the
    /// key the rule produces, and whose own key no rule was applied to.
    /// For a namespace rule: owned entities, not namespaces, inside the
    /// unmapped namespace the rule produces, whose key is not kept
    /// (`spec/verify.md` §9.2).
    pub direct: u64,
    /// For a namespace rule: entities whose namespace chain the rule
    /// mapped and whose key is in `Declared.kept`; 0 for any other rule
    /// (`spec/verify.md` §9.2, WP-111).
    pub kept_via: u64,
}

/// Section 25, `rule_use` (`spec/verify.md` §9.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleUse {
    /// The `rules_id` of the table the walk had.
    pub rules: Digest,
    /// Sorted by `rule`, no two equal; only rules with a count not 0.
    pub uses: Vec<Use>,
    /// The keys of `Declared.kept` some entity of the walk has, as indices
    /// into that list, strictly ascending (`spec/verify.md` §9.2: a kept
    /// key no observer has is reported).
    pub kept: Vec<u64>,
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

/// Section 33, `rebuilt_use` (`spec/verify.md` §9.1, WP-118 R6.4): what
/// one observer's walk under rules said of the run's `rebuilt` entries,
/// and the read-only files that cut an include chain (the hint, R1.3).
/// Present only in an `ok` verify shard under a rule table that is not
/// empty, and only when not every list is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RebuiltUse {
    /// The entries relied on, as indices into `Declared.files.rebuilt`;
    /// strictly ascending.
    pub relied: Vec<u64>,
    /// Strictly ascending by `(file, rule)`.
    pub declares: Vec<Declares>,
    /// Subject paths of the read-only files that cut an include chain and
    /// pass the job's filter (R1.3 rules 2 and 4); strictly ascending, at
    /// most [`RebuiltUse::HINTS_MAX`].
    pub hints: Vec<Vec<u8>>,
    /// How many more there were; 0 unless `hints` has
    /// [`RebuiltUse::HINTS_MAX`].
    pub hints_more: u64,
}

impl RebuiltUse {
    /// The most hints a shard keeps (R1.3).
    pub const HINTS_MAX: usize = 16;

    /// Every list empty and `hints_more` 0: the section is not written.
    pub fn is_empty(&self) -> bool {
        self.relied.is_empty()
            && self.declares.is_empty()
            && self.hints.is_empty()
            && self.hints_more == 0
    }
}

/// One row of [`RebuiltUse::declares`]: how many entities one rebuilt file
/// declares that rule `rule` gave their key (`mapped`), or whose key is the
/// one the rule produces, kept by the walk (`left`, not in
/// `Declared.kept`; `left_kept`, in it).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Declares {
    /// The rebuilt file's subject path.
    pub file: Vec<u8>,
    /// The rule's index in `Declared.rules`.
    pub rule: u64,
    pub mapped: u64,
    pub left: u64,
    pub left_kept: u64,
}

/// Section 34, `existing_use` (`spec/verify.md` §9.1, WP-144): which
/// entries of `Declared.existing` and `Declared.existing_local` some
/// entity of this observer's walk matched. Present only in an `ok` verify
/// shard, and only when at least one list is not empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ExistingUse {
    /// Indices into `Declared.existing`; strictly ascending.
    pub existing: Vec<u64>,
    /// Indices into `Declared.existing_local`; strictly ascending.
    pub local: Vec<u64>,
}

impl ExistingUse {
    /// Both lists empty: the section is not written.
    pub fn is_empty(&self) -> bool {
        self.existing.is_empty() && self.local.is_empty()
    }
}
