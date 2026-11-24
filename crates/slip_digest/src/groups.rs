//! Groups, detail and counters (`spec/digest.md` §11, §12).

use std::collections::BTreeMap;

use slip_model::digests::{
    Digests, Group, GroupClass, GroupSum, SCHEME, Unit, group_digest, tu_digest,
};
use slip_model::{Counter, Digest};

use crate::units::Written;
use crate::{Cause, Detail};

/// What the counters count, after grouping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub units: u64,
    pub derived: u64,
    pub groups: u64,
    pub indeterminate: u64,
}

/// The `digests` section of the written units.
pub(crate) fn build(written: &[Written], detail: &Detail<'_>) -> (Digests, Totals) {
    let mut by_group: BTreeMap<(GroupClass, Vec<u8>), Vec<Unit>> = BTreeMap::new();
    for u in written {
        let flags = if u.cause.is_some() { Unit::INDETERMINATE } else { 0 };
        // The mark (`emitted.rs`, WP-155): part of no digest.
        let flags = if u.emitted { flags | Unit::EMITTED } else { flags };
        by_group.entry((u.class, u.file.clone())).or_default().push(Unit {
            key: u.key,
            digest: u.digest,
            flags,
        });
    }
    let differing: Option<Vec<GroupSum>> = match detail {
        Detail::Differing(sums) => {
            let mut v = sums.to_vec();
            v.sort();
            Some(v)
        }
        Detail::None | Detail::All | Detail::Groups(_) => None,
    };
    let mut totals = Totals::default();
    let mut groups: Vec<Group> = Vec::with_capacity(by_group.len());
    for ((class, file), mut units) in by_group {
        units.sort();
        // Two units of one group with the same key and digest are one
        // unit (§11); it is indeterminate if either is.
        units.dedup_by(|b, a| {
            if a.key == b.key && a.digest == b.digest {
                a.flags |= b.flags;
                true
            } else {
                false
            }
        });
        let count = units.len() as u64;
        let indeterminate = units.iter().filter(|u| u.is_indeterminate()).count() as u64;
        let digest = group_digest(&units);
        let mut g = Group { class, file, count, indeterminate, digest, units: None };
        let keep = match detail {
            Detail::None => false,
            Detail::All => true,
            Detail::Groups(list) => list.iter().any(|(c, f)| *c == g.class && *f == g.file),
            Detail::Differing(_) => {
                g.indeterminate > 0
                    || differing.as_ref().is_none_or(|v| v.binary_search(&g.sum()).is_err())
            }
        };
        if keep {
            g.units = Some(units);
        }
        totals.units += count;
        if class == GroupClass::Derived {
            totals.derived += count;
        }
        totals.indeterminate += indeterminate;
        totals.groups += 1;
        groups.push(g);
    }
    let tu = tu_digest(&groups);
    (Digests { scheme: SCHEME, tu, groups }, totals)
}

/// The counters of §12, plus `digest.indeterminate.<cause>` and
/// `digest.indeterminate.pending.<area>` (one per indeterminate key, by
/// its first cause), sorted by name.
pub(crate) fn counters(
    totals: &Totals,
    collisions: u64,
    bytes: u64,
    causes: &[(Digest, Cause)],
) -> Vec<Counter> {
    let mut by_name: BTreeMap<String, u64> = BTreeMap::new();
    by_name.insert("digest.units".into(), totals.units);
    by_name.insert("digest.units.derived".into(), totals.derived);
    by_name.insert("digest.groups".into(), totals.groups);
    by_name.insert("digest.indeterminate".into(), totals.indeterminate);
    by_name.insert("digest.collisions".into(), collisions);
    by_name.insert("digest.bytes".into(), bytes);
    for (_, c) in causes {
        *by_name.entry(format!("digest.indeterminate.{}", c.name())).or_default() += 1;
        if let Cause::Pending(area) = c {
            *by_name.entry(format!("digest.indeterminate.pending.{}", area.name())).or_default() +=
                1;
        }
    }
    by_name.into_iter().map(|(name, value)| Counter { name, value }).collect()
}
