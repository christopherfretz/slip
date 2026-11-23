//! Atomic groups (`spec/edits.md` §4.4, §5.5): the closure of ties across
//! observers, rejection of a whole group, an `indivisible` request, the
//! `groups` list of the set. Shards written by hand; each test names its
//! section.

mod common;

use common::*;
use slip_model::editset::{Cause, SetState};

const A: &str = "/s/a.cc";

fn x(lo: u64) -> C {
    replace(A, lo, lo + 1, vec![lit("x")])
}

/// §4.4, §5.5: a group seen whole by one observer and in part by another
/// is one group; a contested member rejects all of it and nothing else.
#[test]
fn whole_and_part_and_a_rejected_member() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(x(0)).claim(x(10)).claim(x(20)).group(&[0, 1, 2]).claim(x(50)),
            ShardB::new(1).claim(x(0)).claim(x(10)).group(&[0, 1]),
        ],
        "whole-part",
    );
    assert_eq!(set.groups, vec![vec![0, 1, 2]]);
    assert_eq!(counter(&set, "reduce.groups"), 2, "the group and the edit tied to nothing");

    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(x(0)).claim(x(10)).claim(x(20)).group(&[0, 1, 2]).claim(x(50)),
            ShardB::new(1).claim(x(0)).claim(x(10)).group(&[0, 1]),
            ShardB::new(2).claim(keep(A, 20, 21)),
        ],
        "whole-part-rejected",
    );
    assert_eq!(edits(&set), vec![e(A, 50, 51)], "the neighbour stays");
    assert_eq!(causes(&set), vec![Cause::Contested], "the other members have no refusal of their own");
    assert!(set.groups.is_empty());
    assert_eq!((counter(&set, "reduce.groups"), counter(&set, "reduce.groups.rejected")), (2, 1));
}

/// §5.5: two groups joined by a third observer's group are one.
#[test]
fn two_groups_joined_by_a_third() {
    let shards = |join: bool| {
        let mut v = vec![
            ShardB::new(0).claim(x(0)).claim(x(10)).group(&[0, 1]),
            ShardB::new(1).claim(x(20)).claim(x(30)).group(&[0, 1]),
            ShardB::new(2).claim(keep(A, 30, 31)),
        ];
        if join {
            v.push(ShardB::new(3).claim(x(10)).claim(x(20)).group(&[0, 1]));
        }
        v
    };
    let apart = ok(&script(), shards(false), "apart");
    assert_eq!(edits(&apart), vec![e(A, 0, 1), e(A, 10, 11)]);
    assert_eq!(apart.groups, vec![vec![0, 1]]);
    let joined = ok(&script(), shards(true), "joined");
    assert_eq!(joined.state, SetState::Refused);
    assert!(joined.edits.is_empty());
    assert_eq!(causes(&joined), vec![Cause::Contested]);
}

/// §5.5: the `groups` list holds every group of two or more, sorted.
#[test]
fn the_groups_list() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(x(40)).claim(x(0)).group(&[0, 1]),
            ShardB::new(1).claim(x(10)).claim(x(30)).claim(x(20)).group(&[0, 1, 2]),
        ],
        "list",
    );
    assert_eq!(edits(&set).len(), 5);
    assert_eq!(set.groups, vec![vec![0, 4], vec![1, 2, 3]]);
}

/// §5.2, §5.5: a member turned into a `keep` by selection leaves its group;
/// the rest of the group stands.
#[test]
fn a_member_turned_keep_leaves_the_group() {
    let set = ok(
        &script(),
        vec![ShardB::new(0)
            .candidate(d(9))
            .claim(x(0).subjects(&[d(9)]))
            .claim(x(10))
            .claim(x(20))
            .group(&[0, 1, 2])],
        "turned",
    );
    assert_eq!(edits(&set), vec![e(A, 10, 11), e(A, 20, 21)]);
    assert_eq!(set.groups, vec![vec![0, 1]]);
    assert_eq!(set.kept.len(), 1);
}

/// §5.5: under an `indivisible` request every edit is one group: one
/// rejected site rejects them all, and `groups` stays empty.
#[test]
fn indivisible() {
    let seed = |i| ShardB::new(i).seed(d(1));
    let set = ok(
        &rename(),
        vec![seed(0).claim(x(0).subjects(&[d(1)])).claim(x(10).subjects(&[d(1)])), seed(1).claim(x(20).subjects(&[d(1)]))],
        "indivisible",
    );
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.edits.len(), 3);
    assert!(set.groups.is_empty());
    assert_eq!((counter(&set, "reduce.groups"), counter(&set, "reduce.groups.rejected")), (1, 0));

    let set = ok(
        &rename(),
        vec![
            seed(0).claim(x(0).subjects(&[d(1)])).claim(x(10).subjects(&[d(1)])),
            seed(1).claim(x(20).subjects(&[d(1)])),
            seed(2).claim(keep(A, 20, 21)),
        ],
        "indivisible-rejected",
    );
    assert_eq!(set.state, SetState::Refused);
    assert!(set.edits.is_empty());
    assert_eq!(causes(&set), vec![Cause::Contested]);
    assert_eq!((counter(&set, "reduce.groups"), counter(&set, "reduce.groups.rejected")), (1, 1));
}

/// §5.5, landed WP-062 §5: under an `indivisible` request a refused site
/// with no `replace` claim also rejects every edit.
#[test]
fn indivisible_refused_site() {
    let set = ok(
        &rename(),
        vec![ShardB::new(0)
            .seed(d(1))
            .claim(x(0).subjects(&[d(1)]))
            .claim(refuse(A, 30, 31).subjects(&[d(1)]))],
        "indivisible-refuse",
    );
    assert_eq!(set.state, SetState::Refused);
    assert_eq!(causes(&set), vec![Cause::SiteRefused]);
}

/// §5.5: a group's edit at a site rejected by geometry (`overlap`) takes
/// the group with it; a site rejected for crossing a site of a rejected
/// group stays rejected.
#[test]
fn geometry_rejects_groups() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(x(0)).claim(replace(A, 10, 14, vec![lit("y")])).group(&[0, 1]),
            ShardB::new(1).claim(replace(A, 12, 16, vec![lit("z")])),
            ShardB::new(2).claim(x(50)),
        ],
        "geometry-groups",
    );
    refusal_at(&set, Cause::Overlap, A, 10, 14);
    refusal_at(&set, Cause::Overlap, A, 12, 16);
    assert_eq!(edits(&set), vec![e(A, 50, 51)]);
}
