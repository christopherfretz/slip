//! The declared changes of a run (`spec/verify.md` §2): the rule table and
//! the three exemption lists. Work package WP-022.
//!
//! Plain data. Its container, kind `DECL`, version 1, one required section
//! `declared` (id 1), and its JSON rendering are implemented in `formats`.

use std::fmt;

use crate::Digest;
use crate::keys::{Rule, RuleError, RuleTable, SortError};

/// Declared changes (`spec/verify.md` §2). Every list is strictly
/// ascending: rules by the bytes of their `(parent, kind, name, disc)`,
/// keys by their bytes. A key may be in more than one exemption list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Declared {
    rules: RuleTable,
    changed: Vec<Digest>,
    created: Vec<Digest>,
    removed: Vec<Digest>,
}

/// Lists that cannot be declared changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeclaredError {
    /// Element `index` of `list` (`"rules"`, `"changed"`, `"created"` or
    /// `"removed"`) is not above the one before it. For `rules`, an equal
    /// one is [`RuleError::Duplicate`] instead.
    NotAscending { list: &'static str, index: usize },
    /// The rules' error.
    Rules(RuleError),
}

impl fmt::Display for DeclaredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclaredError::NotAscending { list, index } => {
                write!(f, "`{list}` is not strictly ascending at index {index}")
            }
            DeclaredError::Rules(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for DeclaredError {}

/// The index of the first element of `list` not above the one before it.
pub(crate) fn first_not_ascending(list: &[Digest]) -> Option<usize> {
    (1..list.len()).find(|&i| list[i - 1] >= list[i])
}

impl Declared {
    /// No declared change: the claim is plain equality (`spec/verify.md` §2).
    pub fn none() -> Declared {
        Declared::default()
    }

    /// Sorts nothing: every list must already be strictly ascending. `Err`
    /// names the list and the index that is not, or the rules' error (a
    /// name no reader accepts; two rules with one match, as indices `i - 1`
    /// and `i`). Checked in the order rules, changed, created, removed.
    pub fn new(
        rules: Vec<Rule>,
        changed: Vec<Digest>,
        created: Vec<Digest>,
        removed: Vec<Digest>,
    ) -> Result<Declared, DeclaredError> {
        let rules = RuleTable::from_sorted(rules).map_err(|(index, e)| match e {
            SortError::Invalid(what) => {
                DeclaredError::Rules(RuleError::InvalidName { index, what })
            }
            SortError::Duplicate => {
                DeclaredError::Rules(RuleError::Duplicate { first: index - 1, second: index })
            }
            SortError::Unsorted => DeclaredError::NotAscending { list: "rules", index },
        })?;
        for (list, keys) in [("changed", &changed), ("created", &created), ("removed", &removed)] {
            if let Some(index) = first_not_ascending(keys) {
                return Err(DeclaredError::NotAscending { list, index });
            }
        }
        Ok(Declared { rules, changed, created, removed })
    }

    /// A value whose lists the caller has checked.
    pub(crate) fn from_checked(
        rules: RuleTable,
        changed: Vec<Digest>,
        created: Vec<Digest>,
        removed: Vec<Digest>,
    ) -> Declared {
        Declared { rules, changed, created, removed }
    }

    /// The rule table; its `id()` is the run's `rules_id`.
    pub fn rules(&self) -> &RuleTable {
        &self.rules
    }

    /// Units whose digest may differ.
    pub fn changed(&self) -> &[Digest] {
        &self.changed
    }

    /// Units that may exist only in the post-image.
    pub fn created(&self) -> &[Digest] {
        &self.created
    }

    /// Units that may exist only in the pre-image.
    pub fn removed(&self) -> &[Digest] {
        &self.removed
    }
}
