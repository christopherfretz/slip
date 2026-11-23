//! The `digests` section of a shard (`spec/digest.md` §11): the semantic
//! digest of one translation unit, as groups of units.
//!
//! Plain data, and the two hashes that define a group's and a translation
//! unit's digest. The binary and JSON forms are implemented in `formats`.

use crate::Digest;
use crate::hash::tagged_digest;
use crate::wire::Encoder;

/// The digest scheme this crate reads and writes (`Digests::scheme`).
pub const SCHEME: u64 = 2;

/// Tag of a group's digest: `H("slip.group.v1", list<Unit>)`.
pub const GROUP_TAG: &str = "slip.group.v1";
/// Tag of a translation unit's digest: `H("slip.tu.v1", list<GroupSum>)`.
pub const TU_TAG: &str = "slip.tu.v1";

/// Which units a group holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GroupClass {
    /// Units of entities declared in the group's file.
    Declared = 0,
    /// Units derived from entities declared in the group's file.
    Derived = 1,
    /// The order unit and the namespaces; one group, with an empty file.
    Tu = 2,
}

/// One unit: an entity's key and the digest of what it is.
///
/// Ordered by `(key, digest, flags)`. A group's list is strictly ascending
/// by `(key, digest)`: no two of its units share both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unit {
    /// The entity's mapped `KeyId` (`spec/digest.md` §3.4).
    pub key: Digest,
    pub digest: Digest,
    /// Bit 0: indeterminate. Other bits are zero.
    pub flags: u64,
}

impl Unit {
    /// The flag of a unit whose digest cannot be trusted (`spec/digest.md` §10).
    pub const INDETERMINATE: u64 = 1;

    pub fn is_indeterminate(&self) -> bool {
        self.flags & Unit::INDETERMINATE != 0
    }
}

/// The units of one class that belong to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub class: GroupClass,
    /// The subject path of the file; empty for `GroupClass::Tu` and for builtins.
    pub file: Vec<u8>,
    /// Units in the group.
    pub count: u64,
    /// Units in the group with the indeterminate flag (`spec/digest.md` §11).
    pub indeterminate: u64,
    /// `group_digest` of the group's units.
    pub digest: Digest,
    /// Strictly ascending by `(key, digest)`; present when detail was asked for.
    pub units: Option<Vec<Unit>>,
}

/// The sum of a group (`spec/digest.md` §11 `GroupSum`): everything of it
/// but its unit list. Ordered by `(class, file, count, indeterminate,
/// digest)`, so that a sorted list of sums can be searched.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupSum {
    pub class: GroupClass,
    pub file: Vec<u8>,
    pub count: u64,
    pub indeterminate: u64,
    pub digest: Digest,
}

impl Group {
    /// The group's sum: `(class, file, count, indeterminate, digest)`.
    pub fn sum(&self) -> GroupSum {
        GroupSum {
            class: self.class,
            file: self.file.clone(),
            count: self.count,
            indeterminate: self.indeterminate,
            digest: self.digest,
        }
    }
}

/// The `digests` section. `groups` is sorted by `(class, file)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digests {
    pub scheme: u64,
    /// `tu_digest` of `groups`.
    pub tu: Digest,
    pub groups: Vec<Group>,
}

/// `H("slip.group.v1", list<Unit>)` over units sorted and without
/// duplicates, which the caller ensures.
pub fn group_digest(units: &[Unit]) -> Digest {
    let mut e = Encoder::new();
    e.count(units.len());
    for unit in units {
        e.digest(&unit.key).digest(&unit.digest).uv(unit.flags);
    }
    tagged_digest(GROUP_TAG, e.as_bytes())
}

/// `H("slip.tu.v1", list<GroupSum>)` over groups sorted by `(class, file)`,
/// which the caller ensures. A group's sum is `(class, file, count,
/// indeterminate, digest)`; its unit list is not part of it.
pub fn tu_digest(groups: &[Group]) -> Digest {
    let mut e = Encoder::new();
    e.count(groups.len());
    for group in groups {
        e.uv(group.class as u64)
            .bytes(&group.file)
            .uv(group.count)
            .uv(group.indeterminate)
            .digest(&group.digest);
    }
    tagged_digest(TU_TAG, e.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(byte: u8) -> Digest {
        Digest([byte; 32])
    }

    // spec/digest.md §11: a group's digest covers key, digest and flags of
    // every unit; the translation unit's covers each group's sum and not its
    // unit list.
    #[test]
    fn digests_cover_what_the_spec_says() {
        let a = Unit { key: d(1), digest: d(2), flags: 0 };
        let b = Unit { key: d(3), digest: d(4), flags: 0 };
        let base = group_digest(&[a, b]);
        assert_ne!(base, group_digest(&[a]));
        assert_ne!(base, group_digest(&[a, Unit { key: d(5), ..b }]));
        assert_ne!(base, group_digest(&[a, Unit { digest: d(5), ..b }]));
        assert_ne!(base, group_digest(&[a, Unit { flags: Unit::INDETERMINATE, ..b }]));
        assert_ne!(group_digest(&[]), group_digest(&[a]));

        let g = Group {
            class: GroupClass::Declared,
            file: b"/s/a.h".to_vec(),
            count: 2,
            indeterminate: 0,
            digest: base,
            units: None,
        };
        let tu = tu_digest(std::slice::from_ref(&g));
        assert_eq!(tu, tu_digest(&[Group { units: Some(vec![a, b]), ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[Group { class: GroupClass::Derived, ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[Group { file: b"/s/b.h".to_vec(), ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[Group { count: 3, ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[Group { indeterminate: 1, ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[Group { digest: d(9), ..g.clone() }]));
        assert_ne!(tu, tu_digest(&[]));
    }

    // spec/digest.md §11 `GroupSum`: every field of the group but `units`.
    #[test]
    fn a_sum_is_the_group_without_its_units() {
        let g = Group {
            class: GroupClass::Derived,
            file: b"/s/a.h".to_vec(),
            count: 3,
            indeterminate: 1,
            digest: d(7),
            units: Some(vec![Unit { key: d(1), digest: d(2), flags: 0 }]),
        };
        let s = g.sum();
        assert_eq!(
            s,
            GroupSum {
                class: GroupClass::Derived,
                file: b"/s/a.h".to_vec(),
                count: 3,
                indeterminate: 1,
                digest: d(7),
            }
        );
        assert_eq!(s, Group { units: None, ..g.clone() }.sum());
        assert_ne!(s, Group { count: 4, ..g.clone() }.sum());
        assert_ne!(s, Group { indeterminate: 0, ..g.clone() }.sum());
        assert_ne!(s, Group { digest: d(8), ..g.clone() }.sum());
        assert_ne!(s, Group { file: Vec::new(), ..g.clone() }.sum());
        assert_ne!(s, Group { class: GroupClass::Tu, ..g }.sum());
    }
}
