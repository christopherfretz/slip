//! A rule that matches nothing changes nothing (`spec/digest.md` §3.4,
//! rule "Inert rules"; WP-131 item 3): for every source of every pair
//! directory (`tests/equal`, `differ`, `demand`, `indeterminate`,
//! `resolved`, pending ones included) and both subjects of every case of
//! `tests/rules`, the walk under a table none of whose rules matches a key
//! of that translation unit gives the `Output` of the walk under the empty
//! table, compared in full: the units (keys, digests, flags, classes and
//! files of every group), the counters, the causes, the names, and the
//! rule use (which is empty: no rule is used, `via` and `direct` 0).
//!
//! Each source is parsed once and walked three times on that parse
//! (`common::with_side`), on the runners' pool:
//!
//! - under the empty table, every file owned;
//! - under [`namespace_table`] (one namespace rule), every file owned;
//! - under [`every_kind_table`] (a rule of each kind `tests/common/rules.rs`
//!   writes: namespace, class, function, member, variable, and a member
//!   conversion function), with every file of the source but its main
//!   file read-only, so that the ownership branches (`owned.rs`, the
//!   namespace views of `keys.rs`) are taken where a source has headers.
//!
//! One counter is excluded from the equality: `digest.bytes` (`spec/digest.md`
//! §12, the bytes of the streams hashed) measures the walk's work, not
//! what it found, and a walk under a non-empty table does work the walk
//! under the empty one does not, whether or not a rule matches: a
//! namespace keyed in the view of a member of the other ownership
//! (`namespace_parent`, whenever a file is read-only), and a pattern's or
//! a template's key computed again for its mapped name (`mapped_name`).
//! It must not be smaller. Every other counter is compared. The records (`records.rs`)
//! are not walked again: a record is a unit of the walk (its id, kind,
//! class, name and flag are in the `Output` compared) and its `Key`, which
//! `records` checks against the id, is the preimage of that id.

mod common;

use std::path::{Path, PathBuf};

use common::Side;
use slip_digest::{Detail, Input, Options, Output, ReadOnly, ReadOnlyFiles};
use slip_model::Digest;
use slip_model::keys::{Key, KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;

/// The roots whose directories are walked, in order.
const ROOTS: [&str; 6] = ["equal", "differ", "demand", "indeterminate", "resolved", "rules"];

/// A name no source of the crate's tests declares.
fn id(s: &str) -> Name {
    Name::Identifier(format!("slip_inert_{s}").into_bytes())
}

/// A parent no source declares: the key of a class of that name at the
/// global namespace.
fn absent_class() -> Parent {
    Parent::Unit(
        Key { kind: KeyKind::Class, parent: Parent::Root, name: id("class"), disc: None }.id(),
    )
}

/// A discriminator no function has (a digest is not chosen by hand in
/// any walk).
const ABSENT_DISC: Digest = Digest([0x5a; 32]);

fn rule(parent: Parent, kind: KeyKind, name: Name, disc: Option<Digest>, to: &str) -> Rule {
    Rule { parent, kind, name, disc, to_parent: parent, to_name: id(to) }
}

/// One namespace rule, on a namespace of the global namespace that no
/// source declares.
fn namespace_table() -> RuleTable {
    RuleTable::new(vec![rule(Parent::Root, KeyKind::Namespace, id("ns"), None, "ns_old")])
        .expect("a table")
}

/// A rule of each kind: a namespace, a class, a function, a member
/// function and a member conversion function of a class no source
/// declares (with names sources do use: `f`, `operator int`'s form), a
/// variable. None matches a key of any source.
fn every_kind_table() -> RuleTable {
    RuleTable::new(vec![
        rule(Parent::Root, KeyKind::Namespace, id("ns"), None, "ns_old"),
        rule(Parent::Root, KeyKind::Class, id("class"), None, "class_old"),
        rule(Parent::Root, KeyKind::Function, id("fn"), Some(ABSENT_DISC), "fn_old"),
        rule(
            absent_class(),
            KeyKind::Function,
            Name::Identifier(b"f".to_vec()),
            Some(ABSENT_DISC),
            "f_old",
        ),
        rule(
            absent_class(),
            KeyKind::Function,
            Name::Conversion(ABSENT_DISC),
            Some(ABSENT_DISC),
            "conv_old",
        ),
        rule(Parent::Root, KeyKind::Variable, id("var"), None, "var_old"),
    ])
    .expect("a table")
}

fn walk(input: &Input<'_>, rules: &RuleTable, read_only: ReadOnly<'_>) -> Output {
    slip_digest::digest(
        input,
        &Options { rules, read_only, detail: Detail::All, names: true, rule_use: true },
    )
}

/// The one counter compared as "no smaller" rather than "equal".
const BYTES: &str = "digest.bytes";

/// The subject root the read-only files are named under.
const ROOT: &str = "/inert";

/// What differs between `got` and `want`, the walk under the empty table,
/// as lines naming units; empty when the two are equal.
fn differences(got: &Output, want: &Output) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = |o: &Output| o.counters.iter().find(|c| c.name == BYTES).map(|c| c.value);
    if bytes(got) < bytes(want) {
        out.push(format!("{BYTES}: {:?} (without rules {:?})", bytes(got), bytes(want)));
    }
    let strip = |o: &Output| Output {
        counters: o.counters.iter().filter(|c| c.name != BYTES).cloned().collect(),
        ..o.clone()
    };
    let (got, want) = (&strip(got), &strip(want));
    if got == want {
        return out;
    }
    let (g, w) = (common::index(got), common::index(want));
    let name = |o: &Output, k: &Digest| {
        o.names.iter().find(|(x, _)| x == k).map_or_else(|| format!("{k:?}"), |(_, n)| n.clone())
    };
    for (k, u) in &w {
        match g.get(k) {
            Some(v) if v == u => {}
            Some(v) => out.push(format!("changed {}: {u:?} -> {v:?}", name(want, k))),
            None => out.push(format!("only without rules: {}", name(want, k))),
        }
    }
    for k in g.keys().filter(|k| !w.contains_key(*k)) {
        out.push(format!("only with rules: {}", name(got, k)));
    }
    if got.counters != want.counters {
        for c in got.counters.iter().filter(|c| !want.counters.contains(c)) {
            let w = want.counters.iter().find(|w| w.name == c.name).map(|w| w.value);
            out.push(format!("counter {}: {} (without rules {w:?})", c.name, c.value));
        }
    }
    if got.causes != want.causes {
        out.push("causes differ".into());
    }
    if got.names != want.names {
        out.push("names differ".into());
    }
    if !got.rule_use.is_empty() {
        out.push(format!("rule use: {:?}", got.rule_use));
    }
    if out.is_empty() {
        out.push("the digests differ (groups or files)".into());
    }
    out
}

/// The files of a source but its main file, read-only: the headers of
/// the directory (and the runner's `side.h`), under [`ROOT`].
fn headers_read_only(input: &Input<'_>) -> ReadOnlyFiles {
    let mut readonly: Vec<Vec<u8>> = input
        .sm
        .file_ids()
        .map(|f| (input.subject_path)(f))
        .filter(|p| !p.is_empty() && p.as_slice() != b"main.cpp")
        .map(|p| [format!("{ROOT}/").as_bytes(), &p].concat())
        .collect();
    readonly.sort();
    readonly.dedup();
    let o = Ownership { owned: vec![ROOT.as_bytes().to_vec()], readonly };
    let subject = |f| {
        let p = (input.subject_path)(f);
        if p.is_empty() { p } else { [format!("{ROOT}/").as_bytes(), &p].concat() }
    };
    ReadOnlyFiles::new(Some(&o), input.sm, &subject)
}

/// Both sides of `dir`: the differences of each walk under rules from the
/// walk under none, as lines.
fn one(dir: &Path, tables: &[(&str, RuleTable)]) -> Result<(), String> {
    let mut problems = Vec::new();
    for side in [Side::A, Side::B] {
        let found = common::with_side(dir, side, |input| {
            let base = walk(input, &RuleTable::empty(), ReadOnly::NO_FILE);
            let files = headers_read_only(input);
            let mut lines = Vec::new();
            for (i, (label, t)) in tables.iter().enumerate() {
                let ro = if i == 0 { ReadOnly::NO_FILE } else { ReadOnly::files(&files) };
                let got = walk(input, t, ro);
                lines.extend(differences(&got, &base).into_iter().map(|l| format!("{label}: {l}")));
            }
            lines
        });
        match found {
            Ok(lines) => {
                problems.extend(lines.into_iter().map(|l| format!("side {side:?}: {l}")));
            }
            // A side the snippet front end does not parse (an `errors`
            // side of a pending directory) has no walk to compare.
            Err(e) if common::is_pending(dir) => {
                let _ = e;
            }
            Err(e) => problems.push(format!("side {side:?}: {e}")),
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// Every directory of [`ROOTS`], pending ones included.
fn all_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for root in ROOTS {
        v.extend(common::dirs(root));
    }
    if let Some(only) = std::env::var_os("SLIP_DIGEST_ONLY") {
        let only = only.to_string_lossy().into_owned();
        v.retain(|d| common::rel(d).contains(&only));
        eprintln!("SLIP_DIGEST_ONLY={only}: {} directories", v.len());
    }
    v
}

/// `spec/digest.md` §3.4 "Inert rules": every source, both tables.
#[test]
fn a_rule_that_matches_nothing_changes_nothing() {
    let tables =
        [("namespace rule", namespace_table()), ("a rule of each kind", every_kind_table())];
    let dirs = all_dirs();
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let start = std::time::Instant::now();
    let results = common::pool_map(&dirs, threads, |d| one(d, &tables));
    let mut failures = Vec::new();
    for (dir, r) in dirs.iter().zip(results) {
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => failures.push(format!("{}: {e}", common::rel(dir))),
            Err(p) => failures.push(format!("{}: panicked: {p}", common::rel(dir))),
        }
    }
    eprintln!(
        "inert: {} directories, {} failed, {} ms",
        dirs.len(),
        failures.len(),
        start.elapsed().as_millis()
    );
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        dirs.len(),
        failures.join("\n\n")
    );
}
