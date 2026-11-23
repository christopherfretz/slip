//! Pieces and geometry (`spec/edits.md` §5.3, §5.4, §5.5): strict
//! identity, crossing edits and copied ranges, the exchange and the wrap,
//! `cyclic`, `buried`. Shards written by hand; each test names its
//! section.

mod common;

use common::*;
use slip_model::editset::{Cause, SetState};
use slip_model::edits::Piece;

const A: &str = "/s/a.cc";
const B: &str = "/s/b.h";

/// An edit nobody disputes, so that the set is accepted around the ones a
/// test rejects.
fn bystander() -> C {
    replace(A, 900, 901, vec![lit("q")])
}

fn one(claims: Vec<C>, name: &str) -> slip_model::editset::EditSet {
    let mut b = ShardB::new(0).claim(bystander());
    for c in claims {
        b = b.claim(c);
    }
    ok(&script(), vec![b], name)
}

/// §5.3: the same text as a literal and as a spelled piece is two edits.
#[test]
fn identity_literal_against_spelled() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(replace(A, 0, 1, vec![lit("g")])),
            ShardB::new(1).claim(replace(A, 0, 1, vec![spelled(d(1), "g")])).claim(bystander()),
        ],
        "strict-literal",
    );
    let r = refusal_at(&set, Cause::Divergent, A, 0, 1);
    assert_eq!(r.observers, 2);
}

/// §5.3: one text spelled for two keys is two edits.
#[test]
fn identity_spelled_with_another_key() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(replace(A, 0, 1, vec![spelled(d(1), "g")])),
            ShardB::new(1).claim(replace(A, 0, 1, vec![spelled(d(2), "g")])).claim(bystander()),
        ],
        "strict-key",
    );
    refusal_at(&set, Cause::Divergent, A, 0, 1);
    assert_eq!(edits(&set), vec![e(A, 900, 901)]);
}

/// §5.3: a copy is compared by its file's path, not its index in a shard.
#[test]
fn identity_of_a_copy_is_by_path() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(replace(A, 0, 1, vec![copy(B, 0, 4)])),
            ShardB::new(1).claim(keep("/s/0.h", 0, 1)).claim(replace(A, 0, 1, vec![copy(B, 0, 4)])),
        ],
        "copy-by-path",
    );
    assert_eq!(set.edits.len(), 1);
    assert_eq!(set.edits[0].observers, 2);
    let Piece::Copy(p) = &set.edits[0].pieces[0] else { panic!("a copy") };
    assert_eq!(set.files[p.file as usize].path, B.as_bytes());
}

/// §5.4 `overlap`: two crossing live sites are both rejected, each naming
/// the other.
#[test]
fn crossing_edits() {
    let set = one(vec![replace(A, 0, 4, vec![lit("x")]), replace(A, 2, 6, vec![lit("y")])], "crossing");
    let a = refusal_at(&set, Cause::Overlap, A, 0, 4);
    let b = refusal_at(&set, Cause::Overlap, A, 2, 6);
    assert_eq!(a.other.map(|o| (o.lo, o.hi)), Some((2, 6)));
    assert_eq!(b.other.map(|o| (o.lo, o.hi)), Some((0, 4)));
    assert_eq!(edits(&set), vec![e(A, 900, 901)]);
}

/// §5.4 `overlap`: an edit that crosses a range another edit copies:
/// both.
#[test]
fn edit_crossing_a_copied_range() {
    let set = one(vec![replace(A, 0, 1, vec![copy(A, 10, 20)]), replace(A, 15, 25, vec![lit("y")])], "cross-copy");
    let x = refusal_at(&set, Cause::Overlap, A, 0, 1);
    let y = refusal_at(&set, Cause::Overlap, A, 15, 25);
    assert_eq!(x.other.map(|o| (o.lo, o.hi)), Some((15, 25)));
    assert_eq!(y.other.map(|o| (o.lo, o.hi)), Some((10, 20)));
}

/// §5.4 `overlap`: an edit that crosses a range it copies itself.
#[test]
fn edit_crossing_its_own_copy() {
    let set = one(vec![replace(A, 10, 20, vec![copy(A, 15, 25)])], "cross-own");
    refusal_at(&set, Cause::Overlap, A, 10, 20);
}

/// §5.4: an edit within, equal to, or around a copied range is no
/// conflict.
#[test]
fn edit_within_equal_to_and_around_a_copied_range() {
    for (lo, hi) in [(12, 13), (10, 20), (5, 25)] {
        let set = one(vec![replace(A, 0, 1, vec![copy(A, 10, 20)]), replace(A, lo, hi, vec![lit("y")])], "copy-ok");
        assert_eq!(set.state, SetState::Accepted);
        assert_eq!(edits(&set), vec![e(A, 0, 1), e(A, lo, hi), e(A, 900, 901)], "[{lo}, {hi})");
    }
}

/// §5.4, §5.5: the exchange of two ranges, with an edit inside one: the
/// inner edit is carried by the other's copy.
#[test]
fn exchange_with_an_edit_inside() {
    let set = one(
        vec![
            replace(A, 0, 10, vec![copy(A, 20, 30)]),
            replace(A, 20, 30, vec![copy(A, 0, 10)]),
            replace(A, 2, 3, vec![lit("z")]),
        ],
        "exchange",
    );
    assert_eq!(set.state, SetState::Accepted, "{:?}", set.refusals);
    assert_eq!(edits(&set), vec![e(A, 0, 10), e(A, 2, 3), e(A, 20, 30), e(A, 900, 901)]);
    assert_eq!(counter(&set, "reduce.edits.nested"), 1);
}

/// §5.4, §5.5: a wrap (an edit that copies its own range) with an edit
/// inside.
#[test]
fn wrap_with_an_edit_inside() {
    let set = one(
        vec![replace(A, 10, 20, vec![lit("("), copy(A, 10, 20), lit(")")]), replace(A, 12, 13, vec![lit("z")])],
        "wrap",
    );
    assert_eq!(set.state, SetState::Accepted, "{:?}", set.refusals);
    assert_eq!(edits(&set), vec![e(A, 10, 20), e(A, 12, 13), e(A, 900, 901)]);
}

/// §5.4 `cyclic`: an edit properly inside a range it copies.
#[test]
fn cyclic_one_edit() {
    let set = one(vec![replace(A, 12, 13, vec![copy(A, 10, 20)])], "cyclic-1");
    let r = refusal_at(&set, Cause::Cyclic, A, 12, 13);
    assert_eq!(r.other, None);
}

/// §5.4 `cyclic`: two edits each inside the other's copied range.
#[test]
fn cyclic_two_edits() {
    let set = one(
        vec![replace(A, 0, 5, vec![copy(A, 20, 30)]), replace(A, 22, 23, vec![copy(A, 0, 10)])],
        "cyclic-2",
    );
    refusal_at(&set, Cause::Cyclic, A, 0, 5);
    refusal_at(&set, Cause::Cyclic, A, 22, 23);
    assert_eq!(edits(&set), vec![e(A, 900, 901)]);
}

/// §5.5 `buried`: an edit inside one that does not copy it is rejected,
/// with the edit around it, each naming the other.
#[test]
fn buried_alone() {
    let set = one(vec![replace(A, 0, 10, vec![lit("x")]), replace(A, 2, 3, vec![lit("y")])], "buried");
    let outer = refusal_at(&set, Cause::Buried, A, 0, 10);
    let inner = refusal_at(&set, Cause::Buried, A, 2, 3);
    assert_eq!(outer.other.map(|o| (o.lo, o.hi)), Some((2, 3)));
    assert_eq!(inner.other.map(|o| (o.lo, o.hi)), Some((0, 10)));
    assert_eq!(edits(&set), vec![e(A, 900, 901)]);
}

/// §5.5 `buried`, repeated: an edit carried only by an edit that is
/// itself rejected as buried's container is buried in the next round.
#[test]
fn buried_repeated() {
    let set = one(
        vec![
            replace(A, 0, 10, vec![copy(A, 20, 30)]),
            replace(A, 3, 4, vec![lit("q")]),
            replace(A, 20, 30, vec![lit("v")]),
            replace(A, 22, 23, vec![lit("y")]),
        ],
        "buried-repeated",
    );
    for (lo, hi) in [(0, 10), (3, 4), (20, 30), (22, 23)] {
        refusal_at(&set, Cause::Buried, A, lo, hi);
    }
    assert_eq!(edits(&set), vec![e(A, 900, 901)]);
}

/// §5.4: an edit of another file is not crossed by a range of this one.
#[test]
fn ranges_of_two_files_do_not_meet() {
    let set = one(vec![replace(A, 0, 4, vec![lit("x")]), replace(B, 2, 6, vec![copy(A, 0, 3)])], "two-files");
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.edits.len(), 3);
}
