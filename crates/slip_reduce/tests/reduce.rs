//! Reduce over shards written by hand (`spec/edits.md` §5, §6.2): selection,
//! the refusals of subjects, the table of §5.4, file agreement, unparsed
//! observers, failed runs, declared changes, `rule_conflict`, `no_edit`,
//! and every `Input` error of §5.1 (`docs/wp/WP-062.md`). Each test names
//! its section.

mod common;

use std::sync::atomic::{AtomicU64, Ordering};

use common::*;
use slip_model::edits::{Reason, Role, Severity, subject_flags, tiers};
use slip_model::editset::{Basis, Cause, SetState, basis_id};
use slip_model::{Digest, Phase, Shard, Status};
use slip_reduce::{Options, ReduceError, Shards, declared, reduce};

const A: &str = "/s/a.cc";
const B: &str = "/s/b.h";

fn input_error(r: Result<slip_reduce::Reduced, ReduceError>) -> (Digest, String) {
    match r {
        Err(ReduceError::Input { observer, message }) => (observer, message),
        other => panic!("expected an Input error, got {other:?}"),
    }
}

fn the_edit(path: &str, lo: u64, hi: u64, text: &str) -> C {
    replace(path, lo, hi, vec![lit(text)])
}

// ---------------------------------------------------------------------------
// §5.2: selection

/// §5.2 rule 2: a chain of links across shards selects every subject on
/// it; a candidate on no chain is not selected, its `replace` becomes a
/// `keep` and its `refuse` is dropped.
#[test]
fn selection_follows_a_chain_and_leaves_an_unlinked_candidate() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .seed(d(1))
                .candidate(d(2))
                .link(d(1), d(2))
                .claim(the_edit(A, 0, 1, "x").subjects(&[d(1)])),
            ShardB::new(1)
                .candidate(d(2))
                .candidate(d(3))
                .link(d(2), d(3))
                .claim(the_edit(A, 10, 11, "y").subjects(&[d(3)])),
            ShardB::new(2)
                .candidate(d(9))
                .claim(the_edit(A, 20, 21, "z").subjects(&[d(9)]))
                .claim(refuse(A, 30, 31).subjects(&[d(9)])),
        ],
        "chain",
    );
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(edits(&set), vec![e(A, 0, 1), e(A, 10, 11)]);
    assert_eq!(set.subjects.iter().map(|s| s.id).collect::<Vec<_>>(), vec![d(1), d(2), d(3)]);
    assert_eq!(set.kept.len(), 1, "the unselected candidate's replace is a keep");
    assert_eq!((set.kept[0].lo, set.kept[0].hi), (20, 21));
    assert!(set.refusals.is_empty(), "the unselected refuse is dropped: {:?}", set.refusals);
    assert_eq!(counter(&set, "reduce.subjects"), 4);
    assert_eq!(counter(&set, "reduce.subjects.selected"), 3);
}

/// §5.2 rule 2: a diamond of links, and a follower selected once every
/// entity of its `of` is.
#[test]
fn selection_through_a_diamond_and_a_follower() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .seed(d(1))
                .candidate(d(2))
                .candidate(d(3))
                .link(d(1), d(2))
                .link(d(1), d(3)),
            ShardB::new(1)
                .candidate(d(2))
                .candidate(d(3))
                .candidate(d(4))
                .link(d(2), d(4))
                .link(d(3), d(4))
                .follower(d(5), &[d(3), d(4)])
                .claim(the_edit(A, 0, 1, "x").subjects(&[d(4), d(5)])),
        ],
        "diamond",
    );
    assert_eq!(edits(&set), vec![e(A, 0, 1)]);
    let ids: Vec<_> = set.subjects.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![d(1), d(2), d(3), d(4), d(5)]);
    let f = set.subjects.iter().find(|s| s.id == d(5)).unwrap();
    assert_eq!((f.role, f.of.clone()), (Role::Follower, vec![d(3), d(4)]));
}

/// §5.2 rule 1: the joined subject is a seed if any record is, flags are
/// or-ed, `display` is the least observer index's.
#[test]
fn subjects_join_role_flags_and_display() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).subject(d(1), Role::Candidate, &[], 0, "first"),
            ShardB::new(1)
                .subject(d(1), Role::Seed, &[], 0, "second")
                .claim(the_edit(A, 0, 1, "x")),
        ],
        "join-subjects",
    );
    let s = &set.subjects[0];
    assert_eq!((s.role, s.display.as_str()), (Role::Seed, "first"));
}

// ---------------------------------------------------------------------------
// §5.2 rule 3: each refusal of a subject

/// §5.2: `no_subject` for a rename with no seed.
#[test]
fn no_subject() {
    let set = ok(&rename(), vec![ShardB::new(0).claim(the_edit(A, 0, 1, "g"))], "no-subject");
    assert_eq!(set.state, SetState::Refused);
    assert_eq!(causes(&set), vec![Cause::NoSubject]);
    assert_eq!((set.refusals[0].subject, set.refusals[0].observers), (None, 0));
    assert!(set.edits.is_empty());
    // A script needs no seed.
    let set =
        ok(&script(), vec![ShardB::new(0).claim(the_edit(A, 0, 1, "g"))], "no-subject-script");
    assert_eq!(set.state, SetState::Accepted);
}

/// §5.2: a selected subject's flag refuses the set, counted over the
/// observers whose shard set it; an unselected candidate's flag does not.
#[test]
fn each_flag_of_a_selected_subject() {
    for (flag, cause) in [
        (subject_flags::READ_ONLY, Cause::SubjectReadOnly),
        (subject_flags::UNSUPPORTED, Cause::SubjectUnsupported),
        (subject_flags::UNREACHABLE, Cause::SubjectUnreachable),
        (subject_flags::NAME_TAKEN, Cause::NameTaken),
    ] {
        let set = ok(
            &rename(),
            vec![
                ShardB::new(0)
                    .seed(d(1))
                    .flags(d(1), flag)
                    .claim(the_edit(A, 0, 1, "g").subjects(&[d(1)])),
                ShardB::new(1).seed(d(1)).claim(the_edit(A, 0, 1, "g").subjects(&[d(1)])),
                ShardB::new(2).seed(d(1)).flags(d(1), flag),
                ShardB::new(3).candidate(d(7)).flags(d(7), flag),
            ],
            "flags",
        );
        assert_eq!(set.state, SetState::Refused, "{cause:?}");
        assert_eq!(causes(&set), vec![cause]);
        let r = &set.refusals[0];
        assert_eq!(
            (r.subject, r.observers, r.first.clone()),
            (Some(d(1)), 2, vec![observer(0), observer(2)])
        );
        assert!(set.edits.is_empty());
    }
}

/// §5.2: `mixed_follower`, a follower with some of its `of` selected.
#[test]
fn mixed_follower() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).seed(d(1)).candidate(d(2)).follower(d(3), &[d(1), d(2)]),
            ShardB::new(1).seed(d(1)).claim(the_edit(A, 0, 1, "x").subjects(&[d(1)])),
        ],
        "mixed-follower",
    );
    assert_eq!(causes(&set), vec![Cause::MixedFollower]);
    assert_eq!((set.refusals[0].subject, set.refusals[0].observers), (Some(d(3)), 1));
    assert_eq!(set.state, SetState::Refused);
}

/// §5.2: `mixed_set`, a `replace` or `refuse` claim with some subjects
/// selected and some not, is a refusal of its site.
#[test]
fn mixed_set() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .seed(d(1))
                .candidate(d(2))
                .claim(the_edit(A, 0, 1, "x").subjects(&[d(1), d(2)]))
                .claim(refuse(A, 5, 6).subjects(&[d(1), d(2)]))
                .claim(the_edit(A, 10, 11, "y").subjects(&[d(1)])),
        ],
        "mixed-set",
    );
    assert_eq!(causes(&set), vec![Cause::MixedSet, Cause::MixedSet]);
    refusal_at(&set, Cause::MixedSet, A, 0, 1);
    refusal_at(&set, Cause::MixedSet, A, 5, 6);
    assert_eq!(edits(&set), vec![e(A, 10, 11)], "the other edit is not in a group with them");
}

// ---------------------------------------------------------------------------
// §5.3, §5.4: the outcome of a site, row by row

/// §5.4 row 1 and §5.3: identical claims of two observers are one live
/// edit with `observers` 2, `votes` summed, `first` in plan order.
#[test]
fn live_identical_claims() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 3, "abc").votes(2).notes(&["n1"])),
            ShardB::new(1).claim(the_edit(A, 0, 3, "abc").notes(&["n0"])),
        ],
        "live",
    );
    assert_eq!(set.edits.len(), 1);
    let x = &set.edits[0];
    assert_eq!((x.observers, x.votes), (2, 3));
    assert_eq!(x.first, vec![observer(0), observer(1)]);
    assert_eq!(x.notes, vec![b"n0".to_vec(), b"n1".to_vec()]);
    assert_eq!(counter(&set, "reduce.edits.shared"), 1);
    assert_eq!(counter(&set, "reduce.edits.max_observers"), 2);
    assert_eq!(counter(&set, "reduce.sites.live"), 1);
}

/// §5.4 row 2: `replace` and `unresolved` at one site is live.
#[test]
fn live_with_unresolved() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).claim(unresolved(A, 0, 1)),
        ],
        "live-unresolved",
    );
    assert_eq!(edits(&set), vec![e(A, 0, 1)]);
    assert!(set.unresolved.is_empty());
}

/// §5.4 row 3: two different `pieces` at one site: `divergent`, counting
/// the observers of both.
#[test]
fn divergent() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).claim(the_edit(A, 0, 1, "y")),
            ShardB::new(2).claim(the_edit(A, 5, 6, "z")),
        ],
        "divergent",
    );
    let r = refusal_at(&set, Cause::Divergent, A, 0, 1);
    assert_eq!(r.observers, 2);
    assert_eq!(edits(&set), vec![e(A, 5, 6)]);
    assert_eq!(counter(&set, "reduce.sites.rejected"), 1);
}

/// §5.4 row 4: `replace` and `keep`: `contested`, counting the observers
/// that kept.
#[test]
fn contested_at_the_site() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(2).claim(keep(A, 0, 1)),
        ],
        "contested",
    );
    let r = refusal_at(&set, Cause::Contested, A, 0, 1);
    assert_eq!((r.observers, r.first.clone(), r.other), (1, vec![observer(2)], None));
    assert_eq!(set.state, SetState::Refused);
    assert_eq!(causes(&set), vec![Cause::Contested], "no no_edit beside a contested");
}

/// §5.4 row 5: a `refuse` rejects the site with its reason, whatever
/// else is there.
#[test]
fn site_refused() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).claim(refuse(A, 0, 1).reason(Reason::MacroBody).in_macro(0, "M")),
            ShardB::new(2).claim(refuse(A, 0, 1).reason(Reason::Pasted)),
        ],
        "site-refused",
    );
    let r = refusal_at(&set, Cause::SiteRefused, A, 0, 1);
    assert_eq!(r.reason, Reason::MacroBody, "the least reason");
    assert_eq!(r.observers, 2, "the observers of the refuse claims");
    assert!(r.macro_site.is_some());
}

/// §5.4 rows 6 and 7: `keep` only, `keep` and `unresolved`, `unresolved`
/// only.
#[test]
fn kept_and_unresolved() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(keep(A, 0, 1)).claim(keep(A, 5, 6)).claim(unresolved(A, 9, 10)),
            ShardB::new(1).claim(unresolved(A, 5, 6)).claim(the_edit(A, 20, 21, "x")),
        ],
        "kept",
    );
    let kept: Vec<_> = set.kept.iter().map(|p| (p.lo, p.hi)).collect();
    let unres: Vec<_> = set.unresolved.iter().map(|p| (p.lo, p.hi)).collect();
    assert_eq!(kept, vec![(0, 1), (5, 6)]);
    assert_eq!(unres, vec![(9, 10)]);
    assert_eq!(edits(&set), vec![e(A, 20, 21)]);
    assert_eq!(counter(&set, "reduce.sites.kept"), 2);
    assert_eq!(counter(&set, "reduce.sites.unresolved"), 1);
}

/// §5.4: a site claimed at tier 1 by one observer and tier 0 by another
/// is live with both bits.
#[test]
fn tiers_are_joined() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 4, 5, "x").in_macro(2, "ID")),
            ShardB::new(1).claim(the_edit(A, 4, 5, "x")),
        ],
        "tiers",
    );
    assert_eq!(set.edits[0].tiers, tiers::T0 | tiers::T1);
    assert!(set.edits[0].macro_site.is_some());
}

/// §5.3: the joined `macro_site` of a claim is the least the observers
/// give, whatever their order.
#[test]
fn macro_site_is_the_least() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 4, 5, "x").in_macro(3, "M")),
            ShardB::new(1).claim(the_edit(A, 4, 5, "x").in_macro(2, "ID")),
        ],
        "least-macro",
    );
    let m = set.edits[0].macro_site.as_ref().expect("a macro site");
    assert_eq!((m.name_start, m.name.as_slice()), (2, b"ID".as_slice()));
    // A tie on `name_start` is broken by the name (§4.3), whatever the
    // order of the shards and of the runs (mutation M36).
    for names in [["M", "ID", "Z"], ["Z", "M", "ID"]] {
        for fan_in in [2, 3, 64] {
            let store = Store::of(
                names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| ShardB::new(i).claim(the_edit(A, 4, 5, "x").in_macro(2, n)))
                    .collect(),
            );
            let o = Options { fan_in, threads: 1, scratch: scratch("macro-tie") };
            let set = reduce(&script(), REQUEST, PLAN, 0, &store, &o).unwrap().edits;
            let m = set.edits[0].macro_site.as_ref().expect("a macro site");
            assert_eq!((m.name_start, m.name.as_slice()), (2, b"ID".as_slice()), "{names:?}");
        }
    }
}

/// §5.3: a claim's notes are the union of the observers', at most eight,
/// the least eight kept, whatever the order of the shards and of the runs
/// (mutation M35).
#[test]
fn at_most_eight_notes() {
    let names: Vec<String> = (0..10).map(|i| format!("n{i}")).collect();
    let want: Vec<Vec<u8>> = names[..8].iter().map(|n| n.as_bytes().to_vec()).collect();
    for reversed in [false, true] {
        for fan_in in [2, 3, 64] {
            let mut order: Vec<&String> = names.iter().collect();
            if reversed {
                order.reverse();
            }
            let shards = order
                .iter()
                .enumerate()
                .map(|(i, n)| ShardB::new(i).claim(the_edit(A, 4, 5, "x").notes(&[n.as_str()])))
                .collect();
            let o = Options { fan_in, threads: 1, scratch: scratch("notes") };
            let set = reduce(&script(), REQUEST, PLAN, 0, &Store::of(shards), &o).unwrap().edits;
            assert_eq!(set.edits[0].notes, want, "reversed {reversed}, fan_in {fan_in}");
        }
    }
}

/// §5.4 first geometry row: a kept range of another site that shares a
/// byte with a live site contests it; `other` names the kept range.
#[test]
fn contested_by_another_range() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 4, "x")).claim(the_edit(A, 10, 12, "y")),
            ShardB::new(1).claim(keep(A, 2, 6)),
        ],
        "contested-other",
    );
    let r = refusal_at(&set, Cause::Contested, A, 0, 4);
    assert_eq!(r.other.map(|o| (o.lo, o.hi)), Some((2, 6)));
    assert_eq!(r.observers, 1);
    assert_eq!(edits(&set), vec![e(A, 10, 12)]);
}

// ---------------------------------------------------------------------------
// §5.4: file agreement

/// §5.4: two observers that give one path different contents in `files`.
#[test]
fn file_differs_in_files() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).file(A, SIZE, d(0xa1)).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).file(A, SIZE, d(0xa2)).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(2).claim(the_edit(B, 0, 1, "y")),
        ],
        "differs-files",
    );
    refusal_at(&set, Cause::FileDiffers, A, 0, 1);
    assert_eq!(edits(&set), vec![e(B, 0, 1)]);
}

/// §5.4: a disagreement seen only in another shard's `inputs`.
#[test]
fn file_differs_in_inputs() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).input(A, SIZE + 1, d(0xa2)).claim(the_edit(B, 0, 1, "y")),
        ],
        "differs-inputs",
    );
    refusal_at(&set, Cause::FileDiffers, A, 0, 1);
    assert_eq!(edits(&set), vec![e(B, 0, 1)]);
}

/// §5.4: a disagreement about a file an edit copies from rejects the edit.
#[test]
fn file_differs_in_a_copied_file() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(replace(A, 0, 1, vec![copy(B, 0, 5)])),
            ShardB::new(1).input(B, SIZE, d(0xb2)).claim(the_edit(A, 50, 51, "y")),
        ],
        "differs-copied",
    );
    refusal_at(&set, Cause::FileDiffers, A, 0, 1);
    assert_eq!(edits(&set), vec![e(A, 50, 51)]);
}

/// §5.4: a file no live site names may differ: no refusal.
#[test]
fn file_differs_only_where_it_matters() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).input(B, 1, d(0xb1)).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).input(B, 2, d(0xb2)),
        ],
        "differs-irrelevant",
    );
    assert_eq!(set.state, SetState::Accepted);
}

// ---------------------------------------------------------------------------
// §5.1: unparsed observers and failed runs

/// §5.1, §6: observers without an `ok` shard are unparsed, in plan order,
/// and the basis is the tagged digest of every `(observer, shard)`.
#[test]
fn unparsed_observers_and_the_basis() {
    let shards = vec![
        ShardB::new(0).claim(the_edit(A, 0, 1, "x")).build(),
        ShardB::new(1).status(Status::Diagnosed).build(),
        ShardB::new(2).claim(the_edit(A, 0, 1, "x")).build(),
    ];
    let ids: Vec<_> = shards.iter().map(id_of).collect();
    let store = Store::new(shards).missing(3);
    let set = run(&script(), &store, "unparsed").unwrap().edits;
    assert_eq!(set.unparsed, vec![observer(1), observer(3)]);
    let basis = vec![
        Basis { observer: observer(0), shard: Some(ids[0]) },
        Basis { observer: observer(1), shard: None },
        Basis { observer: observer(2), shard: Some(ids[2]) },
        Basis { observer: observer(3), shard: None },
    ];
    assert_eq!(set.basis, basis_id(&basis));
    assert_eq!(counter(&set, "reduce.unparsed"), 2);
    assert_eq!(counter(&set, "reduce.shards"), 2);
}

/// §5.1: a failed run contributes its failure and nothing else; the same
/// failure of two observers is one record; with no `ok` shard without a
/// failure the set is refused with `no_observer`.
#[test]
fn failed_runs() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).failed("boom", Some((B, 1, 2))),
            ShardB::new(1).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(2).failed("boom", Some((B, 1, 2))),
        ],
        "failed",
    );
    assert_eq!(set.state, SetState::Accepted);
    assert_eq!(set.failures.len(), 1);
    let f = &set.failures[0];
    assert_eq!((f.observers, f.first.clone()), (2, vec![observer(0), observer(2)]));
    assert_eq!(f.failure.rendered, format!("rendered by {}", observer(0)), "the first observer's");
    assert_eq!(counter(&set, "reduce.failed_runs"), 2);
    assert!(
        set.files.iter().any(|x| x.path == B.as_bytes()),
        "the refusal's subject names its file"
    );

    let set = ok(&script(), vec![ShardB::new(0).failed("boom", None)], "failed-all");
    assert_eq!(set.state, SetState::Refused);
    assert!(causes(&set).contains(&Cause::NoObserver));
    let set = run(&script(), &Store::new(Vec::new()).missing(0), "no-shard").unwrap().edits;
    assert!(causes(&set).contains(&Cause::NoObserver));
}

// ---------------------------------------------------------------------------
// §5.5: no_edit

/// §5.5: a set with no `replace` claim is refused with `no_edit`; so is
/// one whose only `replace` was turned into a `keep` by selection.
#[test]
fn no_edit() {
    let set = ok(&script(), vec![ShardB::new(0).claim(keep(A, 0, 1))], "no-edit");
    assert_eq!(causes(&set), vec![Cause::NoEdit]);
    let set = ok(
        &script(),
        vec![ShardB::new(0).candidate(d(2)).claim(the_edit(A, 0, 1, "x").subjects(&[d(2)]))],
        "no-edit-turned",
    );
    assert_eq!(causes(&set), vec![Cause::NoEdit]);
    assert_eq!(set.kept.len(), 1);
}

// ---------------------------------------------------------------------------
// §6.2: declared changes and rule_conflict

/// §6.2: the declared rules are the `renamed` rules and the rules of the
/// selected subjects; `changed` holds the keys of the entries in force.
#[test]
fn declared_changes() {
    let r = reduce_set(vec![
        ShardB::new(0)
            .seed(d(1))
            .rule(d(1), function_rule("f", "g"))
            .renamed(function_rule("h", "i"))
            .claim(the_edit(A, 0, 1, "g").subjects(&[d(1)]))
            .changed(d(0x20), A, 0, 3, "the callee")
            .changed(d(0x21), A, 10, 13, "rejected"),
        ShardB::new(1)
            .seed(d(1))
            .rule(d(1), function_rule("f", "g"))
            .claim(the_edit(A, 10, 11, "g")),
        ShardB::new(2).claim(keep(A, 10, 11)),
    ]);
    let set = &r.edits;
    assert_eq!(set.state, SetState::Accepted);
    let ins: Vec<_> = set.changed.iter().map(|c| (c.key, c.in_force)).collect();
    assert_eq!(ins, vec![(d(0x20), true), (d(0x21), false)]);
    let decl = r.declared.expect("an accepted set declares");
    assert_eq!(decl.changed(), &[d(0x20)]);
    let rules: Vec<_> = decl.rules().rules().to_vec();
    assert_eq!(rules.len(), 2);
    assert_eq!(set.rules, vec![function_rule("h", "i")], "the set's rules are the renamed rules");
    // WP-110: a `Declared` with rules holds the run's files, the request's.
    assert_eq!(decl.files(), Some(&script().ownership));
    assert_eq!(declared(set, &script().ownership).unwrap(), Some(decl), "one derivation");
    assert_eq!(counter(set, "reduce.rules"), 2);
    assert_eq!(counter(set, "reduce.changed.in_force"), 1);
}

/// §6.2: a `changed` entry is in force where an edit shares a byte with
/// its place; a place that only touches the edit, before or after it, is
/// not (review mutation M09).
#[test]
fn changed_in_force_shares_a_byte() {
    let r = reduce_set(vec![
        ShardB::new(0)
            .claim(the_edit(A, 5, 6, "g"))
            .changed(d(0x20), A, 3, 5, "before")
            .changed(d(0x21), A, 6, 8, "after")
            .changed(d(0x22), A, 4, 6, "over"),
    ]);
    let set = &r.edits;
    assert_eq!(set.state, SetState::Accepted);
    let ins: Vec<_> = set.changed.iter().map(|c| (c.key, c.in_force)).collect();
    assert_eq!(ins, vec![(d(0x20), false), (d(0x21), false), (d(0x22), true)]);
    assert_eq!(counter(set, "reduce.changed.in_force"), 1);
}

fn reduce_set(shards: Vec<ShardB>) -> slip_reduce::Reduced {
    run(&script(), &Store::of(shards), "declared").unwrap()
}

/// §6.2: two rules with one match and different targets conflict; so do
/// two of one target from different names; the rules of a subject count.
#[test]
fn rule_conflict_both_ways() {
    for (name, a, b) in [
        ("one source", function_rule("f", "g"), function_rule("f", "h")),
        ("one target", function_rule("f", "g"), function_rule("h", "g")),
    ] {
        let set = ok(
            &script(),
            vec![
                ShardB::new(0).renamed(a).claim(the_edit(A, 0, 1, "x")),
                ShardB::new(1).renamed(b),
            ],
            "conflict",
        );
        assert_eq!(set.state, SetState::Refused, "{name}");
        assert_eq!(causes(&set), vec![Cause::RuleConflict], "{name}");
        assert!(declared(&set, &script().ownership).unwrap().is_none());
    }
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .seed(d(1))
                .rule(d(1), function_rule("f", "g"))
                .claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).renamed(function_rule("f", "h")),
        ],
        "conflict-subject",
    );
    assert_eq!(causes(&set), vec![Cause::RuleConflict], "a subject's rule against a renamed rule");
}

/// §6.2, `spec/verify.md` §2.1 "One way to each key": two namespace rules
/// of one target (post `n` and post `o`, both called `m`: the pre-image's
/// `m` split in two) conflict, as two rules of any other kind do. The set
/// keeps neither rule.
#[test]
fn two_namespace_rules_of_one_target_refuse() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).renamed(namespace_rule("n", "m")).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).renamed(namespace_rule("o", "m")),
        ],
        "namespaces",
    );
    assert_eq!(set.state, SetState::Refused);
    assert_eq!(causes(&set), vec![Cause::RuleConflict]);
    assert_eq!(set.rules, vec![]);
    assert!(declared(&set, &script().ownership).unwrap().is_none());
}

/// §6.2: its accepted neighbour: one namespace rule, given by one shard
/// and not by the other, or by both, is one rule of an accepted set.
#[test]
fn one_namespace_rule_is_accepted() {
    for (name, other) in [
        ("another shard without it", ShardB::new(1)),
        ("another shard with it", ShardB::new(1).renamed(namespace_rule("n", "m"))),
    ] {
        let set = ok(
            &script(),
            vec![
                ShardB::new(0).renamed(namespace_rule("n", "m")).claim(the_edit(A, 0, 1, "x")),
                other,
            ],
            "namespace",
        );
        assert_eq!(set.state, SetState::Accepted, "{name}");
        assert_eq!(set.rules, vec![namespace_rule("n", "m")], "{name}");
        let declared = declared(&set, &script().ownership).unwrap().unwrap();
        assert_eq!(declared.rules().rules(), &[namespace_rule("n", "m")], "{name}");
    }
}

// ---------------------------------------------------------------------------
// §5.1, §4.7: Input errors, one each, naming the observer

fn expect_input(
    shards: Vec<Shard>,
    request: &slip_model::transform::Transform,
    at: usize,
    says: &str,
) {
    let store = Store::new(shards);
    let (obs, message) = input_error(run(request, &store, "input"));
    assert_eq!(obs, store.observers[at], "{message}");
    assert!(message.contains(says), "{message:?} does not say {says:?}");
}

fn plain(i: usize) -> Shard {
    ShardB::new(i).claim(the_edit(A, 0, 1, "x")).build()
}

/// §5.1: `open` fails.
#[test]
fn input_open_fails() {
    let mut store = Store::new(vec![plain(0), plain(1)]);
    store.entries[1] = Entry::Broken("disk".into());
    let (obs, m) = input_error(run(&script(), &store, "input-open"));
    assert_eq!(obs, observer(1));
    assert!(m.contains("disk"));
}

/// §4.7: bytes the shard reader refuses: a trailer that is not the digest
/// of the bytes before it, and a required section removed.
#[test]
fn input_invalid_shard() {
    let mut bad = plain(1).to_bytes();
    *bad.last_mut().unwrap() ^= 1;
    let header = rewrite(&plain(1).to_bytes(), 1, |_| None);
    for bytes in [bad, header] {
        let store = Store::new(vec![plain(0)]).bytes(observer(1), bytes);
        let (obs, m) = input_error(run(&script(), &store, "input-invalid"));
        assert_eq!(obs, observer(1));
        assert!(m.contains("not valid"), "{m}");
    }
}

/// §5.1: a shard of another observer, phase or round.
#[test]
fn input_wrong_observer_phase_round() {
    let mut s = plain(1);
    s.observer = observer(7);
    let mut store = Store::new(vec![plain(0), s]);
    store.observers[1] = observer(1);
    let (obs, m) = input_error(run(&script(), &store, "input-observer"));
    assert_eq!(obs, observer(1));
    assert!(m.contains("another observer"), "{m}");

    let mut s = plain(1);
    s.phase = Phase::Verify;
    s.edits = None;
    expect_input(vec![plain(0), s], &script(), 1, "phase");
    let mut s = plain(1);
    s.round = 3;
    expect_input(vec![plain(0), s], &script(), 1, "round");
}

/// §5.1: an `ok` shard with no `edits` section.
#[test]
fn input_no_edits_section() {
    let mut s = plain(1);
    s.edits = None;
    expect_input(vec![plain(0), s], &script(), 1, "no edits section");
}

/// §5.1: an `edits` section of another scheme. The shard reader refuses
/// it (`spec/edits.md` §4.7), so reduce does not check it again.
#[test]
fn input_other_scheme() {
    let bytes = rewrite(&plain(1).to_bytes(), 17, |p| {
        assert_eq!(p[0], 1, "the payload opens with uv(scheme)");
        let mut p = p.to_vec();
        p[0] = 2;
        Some(p)
    });
    let store = Store::new(vec![plain(0)]).bytes(observer(1), bytes);
    let (obs, m) = input_error(run(&script(), &store, "input-scheme"));
    assert_eq!(obs, observer(1));
    assert!(m.contains("not valid") && m.contains("scheme"), "{m}");
}

/// §5.1: an `edits` section of another request.
#[test]
fn input_other_transform() {
    let mut s = plain(1);
    s.edits.as_mut().unwrap().transform = d(0x45);
    expect_input(vec![plain(0), s], &script(), 1, "another transform");
}

/// §2.1, §5.1: a `files` path that is not canonical, and `/`.
#[test]
fn input_non_canonical_path() {
    for bad in [&b"/s//a.cc"[..], b"/s/./a.cc", b"/s/../a.cc", b"/"] {
        let mut s = ShardB::new(1).claim(keep(A, 0, 1)).no_inputs().build();
        s.edits.as_mut().unwrap().files[0].path = bad.to_vec();
        let store = Store::new(vec![plain(0), s]);
        let (obs, m) = input_error(run(&script(), &store, "input-path"));
        assert_eq!(obs, observer(1), "{m}");
    }
}

/// §2.1, §5.1, landed WP-062 §2: an `inputs` path that is not canonical.
/// The shard reader checks only that `inputs` ascends, so reduce checks it.
#[test]
fn input_non_canonical_inputs_path() {
    for bad in [&b"/s//a.cc"[..], b"/s/./a.cc", b"/s/../a.cc", b"/", b"s/a.cc"] {
        let mut s = ShardB::new(1).claim(keep(A, 0, 1)).build();
        s.inputs.as_mut().unwrap()[0].path = bad.to_vec();
        s.validate().expect("the shard reader does not check inputs paths");
        let store = Store::new(vec![plain(0), s]);
        let (obs, m) = input_error(run(&script(), &store, "input-inputs-path"));
        assert_eq!(obs, observer(1), "{m}");
        assert!(m.contains("inputs[0] is not a canonical"), "{m}");
    }
}

/// §5.7, landed WP-062 §2: a plan that names one observer twice is an
/// `Input` error at the second index that names it, whether the shards
/// there are read or not, and the least index of every error is named.
#[test]
fn input_one_observer_twice() {
    for fan_in in [2, 3, 64] {
        for threads in [1, 4] {
            let o = |name| Options { fan_in, threads, scratch: scratch(name) };
            // Twice, both read.
            let store = Store::new(vec![plain(0), plain(1), plain(2)])
                .bytes(observer(1), plain(1).to_bytes());
            let opts = o("input-twice");
            let (obs, m) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &opts));
            assert_eq!(obs, observer(1), "{m}");
            assert!(m.contains("at index 1 and again"), "{m}");
            assert_clean(&opts.scratch);
            // Twice, unparsed both times: still an error.
            let store = Store::new(vec![plain(0)]).missing(5).missing(5);
            let (_, m) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &o("i2")));
            assert!(m.contains("at index 1 and again"), "{m}");
            // An earlier error of the first pass is named first; a later one
            // is not.
            let mut early = plain(1);
            early.edits.as_mut().unwrap().transform = d(0x45);
            let store = Store::new(vec![plain(0), early, plain(2), plain(3)])
                .bytes(observer(2), plain(2).to_bytes());
            let (obs, m) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &o("i3")));
            assert_eq!(obs, observer(1), "{m}");
            assert!(m.contains("another transform"), "{m}");
            let mut late = plain(5);
            late.edits.as_mut().unwrap().transform = d(0x45);
            let store = Store::new(vec![plain(0), plain(1), plain(2)])
                .bytes(observer(0), plain(0).to_bytes())
                .bytes(observer(5), late.to_bytes());
            let (obs, m) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &o("i4")));
            assert_eq!(obs, observer(0), "{m}");
            assert!(m.contains("at index 0 and again"), "{m}");
        }
    }
}

/// §5.7: `fan_in` below 2 and `threads` 0 are errors, not panics, and
/// make no temporary file.
#[test]
fn options_out_of_range() {
    let store = Store::new(vec![plain(0)]);
    for (fan_in, threads) in [(1, 1), (0, 1), (2, 0)] {
        let o = Options { fan_in, threads, scratch: scratch("options") };
        match reduce(&script(), REQUEST, PLAN, 0, &store, &o) {
            Err(ReduceError::Options(m)) => assert!(!m.is_empty()),
            other => panic!("fan_in {fan_in}, threads {threads}: {other:?}"),
        }
        assert_clean(&o.scratch);
    }
}

/// §5.1: a `files` entry that disagrees with the shard's `inputs`, or is
/// missing from them.
#[test]
fn input_files_disagree_with_inputs() {
    let mut s = plain(1);
    s.inputs.as_mut().unwrap()[0].size = 3;
    expect_input(vec![plain(0), s], &script(), 1, "disagrees with the shard's inputs");
    let mut s = plain(1);
    s.inputs = Some(Vec::new());
    expect_input(vec![plain(0), s], &script(), 1, "not in the shard's inputs");
}

/// §5.1: a link or a claim that names a key that is no subject of its
/// shard.
#[test]
fn input_unknown_subject() {
    let mut s = ShardB::new(1).seed(d(1)).seed(d(2)).link(d(1), d(2)).build();
    s.edits.as_mut().unwrap().subjects.remove(1);
    expect_input(vec![plain(0), s], &script(), 1, "links[0]");
    let mut s = ShardB::new(1).seed(d(1)).claim(the_edit(A, 0, 1, "x").subjects(&[d(1)])).build();
    s.edits.as_mut().unwrap().subjects.clear();
    expect_input(vec![plain(0), s], &script(), 1, "claims[0]");
}

/// §5.1: a `replace` in a file the request does not own, outside the
/// owned tree or under a read-only entry.
#[test]
fn input_replace_not_owned() {
    let s = ShardB::new(1).claim(the_edit("/t/x.cc", 0, 1, "x")).build();
    expect_input(vec![plain(0), s], &script(), 1, "does not own");
    let s = ShardB::new(1).claim(the_edit("/s/ro/x.h", 0, 1, "x")).build();
    expect_input(vec![plain(0), s], &script(), 1, "does not own");
    // A keep there is fine.
    let s = ShardB::new(1).claim(keep("/s/ro/x.h", 0, 1)).build();
    assert!(run(&script(), &Store::new(vec![plain(0), s]), "input-owned-keep").is_ok());
}

/// §5.1: an identity rule (`spec/verify.md` §2.1, "One way to each key"),
/// in a shard's `renamed` or as a subject's `rule`, is an `Input` error
/// naming the shard: no rule table may hold it, and no observer gives one
/// (`expect_renamed` refuses a rename to the entity's own name).
#[test]
fn input_identity_rule() {
    let s = ShardB::new(1).renamed(function_rule("f", "f")).build();
    expect_input(vec![plain(0), s], &script(), 1, "renamed[0] is a rule no table may hold");
    let s = ShardB::new(1).seed(d(1)).rule(d(1), namespace_rule("n", "n")).build();
    expect_input(vec![plain(0), s], &script(), 1, "subjects[0].rule is a rule no table may hold");
}

/// §5.2 rule 1: two subjects of one id with different rules; the later
/// observer is named.
#[test]
fn input_one_key_two_rules() {
    let a = ShardB::new(0)
        .seed(d(1))
        .rule(d(1), function_rule("f", "g"))
        .claim(the_edit(A, 0, 1, "x"))
        .build();
    let b = ShardB::new(1).candidate(d(9)).build();
    let c = ShardB::new(2).seed(d(1)).rule(d(1), function_rule("f", "h")).build();
    expect_input(vec![a, b, c], &script(), 2, "rule");
}

/// §5.1: the error named is the least observer index's, whatever the
/// threads and `fan_in`.
#[test]
fn input_least_observer_index() {
    for threads in [1, 2, 8] {
        for fan_in in [2, 3, 64] {
            let mut shards: Vec<Shard> = (0..20).map(plain).collect();
            for bad in [13, 5, 17] {
                shards[bad].edits.as_mut().unwrap().transform = d(0x45);
            }
            let mut store = Store::new(shards);
            store.delay = Some(7);
            let o = Options { fan_in, threads, scratch: scratch("input-least") };
            let (obs, _) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &o));
            assert_eq!(obs, observer(5), "threads {threads}, fan_in {fan_in}");
            assert_clean(&o.scratch);
        }
    }
}

/// §5.1, landed WP-062 §1, §2: a shard that is not the one the first pass
/// read, at the second: gone, bytes that no longer read, or other bytes
/// (here a counter more: the same claims, the same files). The id compared
/// is of the bytes each pass read; the store is not asked for one.
#[test]
fn input_shard_changed_between_passes() {
    struct Changing {
        inner: Store,
        later: Option<Vec<u8>>,
        opens: Vec<AtomicU64>,
    }
    impl Shards for Changing {
        fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
            let n = self.opens[index].fetch_add(1, Ordering::Relaxed);
            let s = self.inner.open(index)?;
            Ok(if index == 1 && n > 0 { self.later.clone() } else { s })
        }
        fn len(&self) -> usize {
            self.inner.len()
        }
        fn observer(&self, index: usize) -> Digest {
            self.inner.observer(index)
        }
    }
    let mut torn = plain(1).to_bytes();
    torn.truncate(torn.len() - 1);
    let other = ShardB::new(1).claim(the_edit(A, 0, 1, "x")).counter("api.calls", 1).build();
    for later in [None, Some(torn), Some(other.to_bytes())] {
        let store = Changing {
            inner: Store::new(vec![plain(0), plain(1)]),
            later,
            opens: (0..2).map(|_| AtomicU64::new(0)).collect(),
        };
        let o = options("input-changed");
        let (obs, m) = input_error(reduce(&script(), REQUEST, PLAN, 0, &store, &o));
        assert_eq!(obs, observer(1));
        assert!(m.contains("changed"), "{m}");
        assert_clean(&o.scratch);
        assert_eq!(store.opens[1].load(Ordering::Relaxed), 2, "one open a pass");
    }
}

/// §5.6: reports, `changed` entries and failures are joined by record,
/// with places by path.
#[test]
fn reports_are_joined() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .report(Some((A, 0, 1)), Severity::Review, "look")
                .claim(the_edit(A, 5, 6, "x")),
            ShardB::new(1).report(Some((A, 0, 1)), Severity::Review, "look").report(
                None,
                Severity::Note,
                "n",
            ),
        ],
        "reports",
    );
    assert_eq!(set.reports.len(), 2);
    let look = set.reports.iter().find(|r| r.report.message == b"look").unwrap();
    assert_eq!(look.observers, 2);
    assert_eq!(counter(&set, "reduce.reports.review"), 1);
    assert_eq!(counter(&set, "reduce.reports.note"), 1);
}

/// §6.3: shard counters named `api.*` and `edits.*` are summed, and
/// `api.peak_bytes` is the largest.
#[test]
fn shard_counters_are_summed() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .counter("edits.claims", 2)
                .counter("api.peak_bytes", 10)
                .counter("map.x", 1)
                .claim(the_edit(A, 0, 1, "x")),
            ShardB::new(1).counter("edits.claims", 3).counter("api.peak_bytes", 30),
        ],
        "counters",
    );
    assert_eq!(counter(&set, "edits.claims"), 5);
    assert_eq!(counter(&set, "api.peak_bytes"), 30);
    assert!(set.counters.iter().all(|c| c.name != "map.x"));
    assert!(set.counters.iter().all(|c| c.name != "reduce.runs"), "landed WP-062: not in the set");
    assert_eq!(counter(&set, "reduce.claims"), 1);
    assert_eq!(counter(&set, "reduce.claims.replace"), 1);
}

/// §6, landed WP-062 §1: the basis names the content id of the bytes
/// reduce read, which is not that of the decoded shard's re-encoding when
/// the stored bytes hold an ignorable section.
#[test]
fn basis_is_of_the_bytes_read() {
    use slip_model::formats::{content_id, dump, undump};
    use slip_model::json;
    let s = plain(0);
    let bytes = s.to_bytes();
    let text = dump(&bytes).unwrap();
    let mut v = json::parse(text.as_bytes()).unwrap();
    if let json::Value::Object(top) = &mut v {
        let sections = top.iter_mut().find(|(k, _)| k == "sections").unwrap();
        if let json::Value::Object(m) = &mut sections.1 {
            m.push(("#64".to_owned(), json::parse(br#"{"hex": "00"}"#).unwrap()));
        }
    }
    let stored = undump(&v).unwrap();
    let back = Shard::from_bytes(&stored).unwrap();
    assert_eq!(back, s, "the ignorable section is dropped on decoding");
    assert_ne!(content_id(&stored).unwrap(), content_id(&back.to_bytes()).unwrap());
    let store = Store::new(Vec::new()).bytes(observer(0), stored.clone());
    let set = run(&script(), &store, "basis-bytes").unwrap().edits;
    let basis = vec![Basis { observer: observer(0), shard: Some(content_id(&stored).unwrap()) }];
    assert_eq!(set.basis, basis_id(&basis));
}

/// §5.3, §6, landed WP-062 §4: an observer that made several claims at a
/// site is counted once among the observers of a refusal: two different
/// `replace` claims of one observer (`divergent`), a `keep` and an
/// `unresolved` of one observer (`contested` by another range).
#[test]
fn an_observer_counts_once_at_a_site() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")).claim(the_edit(A, 0, 1, "y")),
            ShardB::new(1).claim(the_edit(A, 0, 1, "x")),
            ShardB::new(2).claim(the_edit(A, 50, 51, "z")),
        ],
        "once-divergent",
    );
    let r = refusal_at(&set, Cause::Divergent, A, 0, 1);
    assert_eq!((r.observers, r.first.clone()), (2, vec![observer(0), observer(1)]));

    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(keep(A, 2, 6)).claim(unresolved(A, 2, 6)),
            ShardB::new(1).claim(the_edit(A, 0, 4, "x")).claim(the_edit(A, 50, 51, "z")),
            ShardB::new(2)
                .candidate(d(9))
                .claim(the_edit(A, 2, 6, "w").subjects(&[d(9)]))
                .claim(keep(A, 2, 6)),
        ],
        "once-contested",
    );
    let r = refusal_at(&set, Cause::Contested, A, 0, 4);
    assert_eq!(r.observers, 2, "observers 0 and 2, each once");
}

/// §5.4's table (mutation M22): `replace`, `keep` and `unresolved` at one
/// site is `contested`, counting the observers of the `keep`.
#[test]
fn replace_keep_and_unresolved_at_one_site() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0).claim(the_edit(A, 0, 1, "x")).claim(the_edit(A, 50, 51, "q")),
            ShardB::new(1).claim(keep(A, 0, 1)),
            ShardB::new(2).claim(unresolved(A, 0, 1)),
        ],
        "replace-keep-unresolved",
    );
    let r = refusal_at(&set, Cause::Contested, A, 0, 1);
    assert_eq!((r.observers, r.first.clone()), (1, vec![observer(1)]));
    assert_eq!(edits(&set), vec![e(A, 50, 51)]);
}

/// §5.2, §5.3 (mutation M38): a claim's `subjects` is the union over its
/// observers, so one observer that names a selected subject and another
/// that names an unselected one for the same edit make `mixed_set`.
#[test]
fn mixed_set_across_observers() {
    let set = ok(
        &script(),
        vec![
            ShardB::new(0)
                .seed(d(1))
                .claim(the_edit(A, 0, 1, "x").subjects(&[d(1)]))
                .claim(the_edit(A, 50, 51, "q")),
            ShardB::new(1).candidate(d(2)).claim(the_edit(A, 0, 1, "x").subjects(&[d(2)])),
        ],
        "mixed-across",
    );
    let r = refusal_at(&set, Cause::MixedSet, A, 0, 1);
    assert_eq!(r.observers, 2);
    assert_eq!(edits(&set), vec![e(A, 50, 51)]);
}
