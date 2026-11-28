//! The declared changes of a run (`spec/verify.md` §2): the rule table,
//! the three exemption lists, `kept`, the run's read-only files, and
//! `existing` / `existing_local` (WP-144). Work packages WP-022, WP-110,
//! WP-118, and WP-144.
//!
//! Plain data. Its container, kind `DECL`, version 2, 3 when the run's
//! files have a `rebuilt` entry (WP-118), or 4 when either `existing`
//! list is not empty (WP-144; version 1 is read), one required section
//! `declared` (id 1), and its JSON rendering are implemented in `formats`.

use std::fmt;

use crate::Digest;
use crate::hash::tagged_digest;
use crate::keys::{Rule, RuleError, RuleTable, SortError};
use crate::transform::Ownership;
use crate::wire::Encoder;

/// The tag of `rules_id` when the run's files are given
/// (`spec/verify.md` §2): `H("slip.rules.v2", rules, files)`.
pub const RULES_V2_TAG: &str = "slip.rules.v2";

/// The tag of `rules_id` when the run's files have a `rebuilt` entry
/// (`spec/verify.md` §2, WP-118 R5.5): `H("slip.rules.v3", rules, owned,
/// readonly, rebuilt)`.
pub const RULES_V3_TAG: &str = "slip.rules.v3";

/// The tag of `rules_id` when `existing` or `existing_local` is not empty
/// (`spec/verify.md` §2, WP-144): `H("slip.rules.v4", rules, owned,
/// readonly, rebuilt, existing, existing_local)`.
pub const RULES_V4_TAG: &str = "slip.rules.v4";

/// One entry of [`Declared::existing_local`] (`spec/verify.md` §2, WP-144):
/// a local entity that was already in the target namespace before the edit,
/// identified by its key and the canonical absolute subject path of the
/// file a declaration of it lies in.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExistingLocal {
    pub key: Digest,
    pub file: Vec<u8>,
}

/// Declared changes (`spec/verify.md` §2). Every list is strictly
/// ascending: rules by the bytes of their `(parent, kind, name, disc)`,
/// keys by their bytes, `existing_local` by `(key, file)`. A key may be in
/// more than one exemption list.
///
/// A value may hold rules and no `files` (what [`Declared::new`] makes):
/// [`Declared::validate`] refuses it, and no container holds one
/// (`spec/verify.md` §2, "`files` … required when `rules` is not empty").
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Declared {
    rules: RuleTable,
    changed: Vec<Digest>,
    created: Vec<Digest>,
    removed: Vec<Digest>,
    kept: Vec<Digest>,
    files: Option<Ownership>,
    existing: Vec<Digest>,
    existing_local: Vec<ExistingLocal>,
}

/// Lists that cannot be declared changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeclaredError {
    /// Element `index` of `list` (`"rules"`, `"changed"`, `"created"`,
    /// `"removed"` or `"kept"`) is not above the one before it. For `rules`, an equal
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

/// The index of the first element of `list` not above the one before it,
/// by `(key, file)`.
pub(crate) fn first_local_not_ascending(list: &[ExistingLocal]) -> Option<usize> {
    (1..list.len())
        .find(|&i| (&list[i - 1].key, &list[i - 1].file) >= (&list[i].key, &list[i].file))
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
            SortError::SameProduced { first } => {
                DeclaredError::Rules(RuleError::SameProduced { first, second: index })
            }
        })?;
        for (list, keys) in [("changed", &changed), ("created", &created), ("removed", &removed)] {
            if let Some(index) = first_not_ascending(keys) {
                return Err(DeclaredError::NotAscending { list, index });
            }
        }
        Ok(Declared {
            rules,
            changed,
            created,
            removed,
            kept: Vec::new(),
            files: None,
            existing: Vec::new(),
            existing_local: Vec::new(),
        })
    }

    /// The same, with `kept` (`spec/verify.md` §2, §9.2): strictly
    /// ascending, or `NotAscending { list: "kept", .. }`.
    pub fn with_kept(self, kept: Vec<Digest>) -> Result<Declared, DeclaredError> {
        if let Some(index) = first_not_ascending(&kept) {
            return Err(DeclaredError::NotAscending { list: "kept", index });
        }
        Ok(Declared { kept, ..self })
    }

    /// The same, with the run's files (`spec/verify.md` §2,
    /// `spec/edits.md` §2.1). Whether they are well formed, and present
    /// exactly when there are rules, is [`Declared::validate`]'s.
    #[must_use]
    pub fn with_files(self, files: Option<Ownership>) -> Declared {
        Declared { files, ..self }
    }

    /// The same, with `existing` and `existing_local` (`spec/verify.md` §2,
    /// WP-144): each strictly ascending, or `NotAscending`. Whether
    /// `existing_local` files are canonical and a namespace rule is present
    /// is [`Declared::validate`]'s.
    pub fn with_existing(
        self,
        existing: Vec<Digest>,
        existing_local: Vec<ExistingLocal>,
    ) -> Result<Declared, DeclaredError> {
        if let Some(index) = first_not_ascending(&existing) {
            return Err(DeclaredError::NotAscending { list: "existing", index });
        }
        if let Some(index) = first_local_not_ascending(&existing_local) {
            return Err(DeclaredError::NotAscending { list: "existing_local", index });
        }
        Ok(Declared { existing, existing_local, ..self })
    }

    /// A value whose lists the caller has checked.
    pub(crate) fn from_checked(
        rules: RuleTable,
        [changed, created, removed, kept]: [Vec<Digest>; 4],
        files: Option<Ownership>,
        existing: Vec<Digest>,
        existing_local: Vec<ExistingLocal>,
    ) -> Declared {
        Declared { rules, changed, created, removed, kept, files, existing, existing_local }
    }

    /// The rule table. The run's `rules_id` is [`Declared::rules_id`],
    /// which is this table's `id()` only when `files` is absent and
    /// `existing` / `existing_local` are empty.
    pub fn rules(&self) -> &RuleTable {
        &self.rules
    }

    /// The run's `rules_id` (`spec/verify.md` §2): `H("slip.rules.v1",
    /// rules)` without `files` (and no existing entry), `H("slip.rules.v2",
    /// rules, files)` with them and empty `rebuilt` and existing lists,
    /// `H("slip.rules.v3", rules, owned, readonly, rebuilt)` when `rebuilt`
    /// is not empty and existing lists are empty (WP-118 R5.5), and
    /// `H("slip.rules.v4", rules, owned, readonly, rebuilt, existing,
    /// existing_local)` when `existing` or `existing_local` is not empty
    /// (WP-144).
    pub fn rules_id(&self) -> Digest {
        let v4 = !self.existing.is_empty() || !self.existing_local.is_empty();
        if v4 {
            let mut e = Encoder::new();
            e.count(self.rules.rules().len());
            for rule in self.rules.rules() {
                rule.encode(&mut e);
            }
            let empty = Vec::new();
            let (owned, readonly, rebuilt) = match &self.files {
                Some(f) => (&f.owned, &f.readonly, &f.rebuilt),
                None => (&empty, &empty, &empty),
            };
            for list in [owned, readonly, rebuilt] {
                e.count(list.len());
                for path in list {
                    e.bytes(path);
                }
            }
            e.count(self.existing.len());
            for key in &self.existing {
                e.digest(key);
            }
            e.count(self.existing_local.len());
            for entry in &self.existing_local {
                e.digest(&entry.key).bytes(&entry.file);
            }
            return tagged_digest(RULES_V4_TAG, e.as_bytes());
        }
        let Some(files) = &self.files else {
            return self.rules.id();
        };
        let mut e = Encoder::new();
        e.count(self.rules.rules().len());
        for rule in self.rules.rules() {
            rule.encode(&mut e);
        }
        let v3 = !files.rebuilt.is_empty();
        let lists: &[&Vec<Vec<u8>>] = if v3 {
            &[&files.owned, &files.readonly, &files.rebuilt]
        } else {
            &[&files.owned, &files.readonly]
        };
        for list in lists {
            e.count(list.len());
            for path in *list {
                e.bytes(path);
            }
        }
        tagged_digest(if v3 { RULES_V3_TAG } else { RULES_V2_TAG }, e.as_bytes())
    }

    /// Program-wide entities and whole namespaces that were already in the
    /// target namespace before the edit (`spec/verify.md` §2, WP-144).
    pub fn existing(&self) -> &[Digest] {
        &self.existing
    }

    /// Local entities that were already in the target namespace before the
    /// edit, each paired with the file a declaration of it lies in
    /// (`spec/verify.md` §2, WP-144).
    pub fn existing_local(&self) -> &[ExistingLocal] {
        &self.existing_local
    }

    /// Owned entities that are, on purpose, members of a namespace a rule
    /// maps onto (`spec/verify.md` §9.2). Carried only: its check is
    /// WP-111's.
    pub fn kept(&self) -> &[Digest] {
        &self.kept
    }

    /// Which files of the run are owned, read-only or rebuilt
    /// (`spec/edits.md` §2.1); present exactly when the rules are not
    /// empty, in a valid value.
    pub fn files(&self) -> Option<&Ownership> {
        self.files.as_ref()
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
