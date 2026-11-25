//! `Ref::views` writes its keys ascending (`spec/digest.md` §4.1,
//! `spec/digest-kinds.md` §2.2; WP-141 mutation 4, its unsorted half).
//! R1 forms the keys in the order of the named namespace's view keys, its
//! own key first; the own key is the dependency view's wherever one
//! opening is read-only, the same in every observer, so no pair of images
//! can show the order (two observers write the same bytes, whatever their
//! include order, and no pre-image has a `Ref::views`). The order is the
//! encoding's, and this test reads it from the stream: the function whose
//! dependent call finds `h` in both parts of `acme` writes `Ref::views`
//! with its two keys ascending, in a case where the own key (`acme`, the
//! dependency's) is the greater, so that the namespace's order would be
//! descending.

use std::collections::BTreeMap;

use slip_digest::{Detail, Input, Options, Output, ReadOnly, ReadOnlyFiles};
use slip_kiln::kiln::source::FileId;
use slip_model::Digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;

/// The dependency's `acme`, read-only, with one overload of `h`.
const DEP: &str = "namespace acme { int h(int); }\n";
/// The owned part of `acme` (moved from `legacy`), with the other.
const OWN: &str = "namespace acme { int h(long); }\n";
/// The function whose dependent call finds `h` in both parts.
const USE: &str = "template <class T> int f(T t) { return acme::h(t); }\n";

const ROOT: &str = "/case";

/// `Ref::views` (`spec/digest.md` §4.1).
const REF_VIEWS: u8 = 6;

/// The post-image's rule of `relocate legacy => :: acme`.
fn table() -> RuleTable {
    let rule = Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: Name::Identifier(b"acme".to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(b"legacy".to_vec()),
    };
    RuleTable::new(vec![rule]).expect("one rule")
}

/// Parses the post-image with `first` and `second` included in that
/// order and gives `f` the input and the run's options: the rule when
/// `rules`, else none; `dep.h` read-only.
fn with_post<R>(
    first: &str,
    second: &str,
    rules: bool,
    f: impl FnOnce(&Input<'_>, &Options<'_>) -> R,
) -> R {
    let main = format!("#include \"{first}\"\n#include \"{second}\"\n{USE}");
    let files = [("main.cpp", main.as_str()), ("dep.h", DEP), ("own.h", OWN)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    let extra = slip_kiln::snippet::Extra::default();
    let table = if rules { table() } else { RuleTable::empty() };
    slip_kiln::snippet::parse_with(&snippet, &extra, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let paths: Vec<Vec<u8>> =
            p.sm.file_ids()
                .map(|f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default())
                .collect();
        let path = |f: FileId| paths.get(f.index()).cloned().unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let o = Ownership {
            owned: vec![ROOT.as_bytes().to_vec()],
            readonly: vec![format!("{ROOT}/dep.h").into_bytes()],
            rebuilt: Vec::new(),
        };
        let subject = |f| {
            let p = (input.subject_path)(f);
            if p.is_empty() { p } else { [format!("{ROOT}/").as_bytes(), &p].concat() }
        };
        let ro = ReadOnlyFiles::new(Some(&o), input.sm, &subject);
        let options = Options {
            rules: &table,
            read_only: ReadOnly::files(&ro),
            detail: Detail::All,
            names: true,
            rule_use: false,
        };
        f(&input, &options)
    })
    .unwrap_or_else(|e| panic!("does not parse: {e}"))
}

fn names(o: &Output) -> BTreeMap<Digest, String> {
    o.names.iter().cloned().collect()
}

/// The `(key, digest)` of the units named like `f`.
fn units_of_f(out: &Output) -> Vec<(Digest, Digest)> {
    let names = names(out);
    let mut f: Vec<(Digest, Digest)> = out
        .digests
        .groups
        .iter()
        .flat_map(|g| g.units.iter().flatten())
        .filter(|u| names.get(&u.key).is_some_and(|n| n.contains(" f(")))
        .map(|u| (u.key, u.digest))
        .collect();
    f.sort();
    f
}

/// The keys of the units of `acme`: its own (`acme`) and its view unit's
/// (named as a part of it, WP-140 review F4).
fn keys_of_acme(out: &Output) -> Vec<Digest> {
    names(out)
        .into_iter()
        .filter(|(_, n)| n == "acme" || n.ends_with("part of acme)"))
        .map(|(k, _)| k)
        .collect()
}

fn counter(o: &Output, name: &str) -> u64 {
    o.counters.iter().find(|c| c.name == name).map_or(0, |c| c.value)
}

#[test]
fn views_are_written_ascending() {
    let out = with_post("dep.h", "own.h", true, slip_digest::digest);
    let f = units_of_f(&out);
    assert_eq!(f.len(), 1, "one unit of f: {f:?}");
    assert_eq!(counter(&out, "digest.ref.namespace_route_unknown"), 0);
    let key_f = f[0].0;
    // The namespace's two view keys: its own (the dependency's `acme`, its
    // key under no rule) and the owned view's (`legacy`, mapped).
    let view_keys = keys_of_acme(&out);
    assert_eq!(view_keys.len(), 2, "acme has a unit in each view: {:?}", names(&out));
    let plain = with_post("dep.h", "own.h", false, slip_digest::digest);
    let own = keys_of_acme(&plain);
    assert_eq!(own.len(), 1, "one acme under no rule");
    let own = own[0];
    assert!(view_keys.contains(&own), "the own key is a view key");
    let other = *view_keys.iter().find(|&&k| k != own).expect("two keys");
    assert!(own > other, "the case has teeth only if the own key is the greater: rename");
    let traced =
        with_post("dep.h", "own.h", true, |i, o| slip_digest::trace::trace(i, o, &[key_f]));
    let t = traced.traces.iter().find(|t| t.key == key_f).expect("f is traced");
    let mut seen = 0;
    for w in t.refs.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let pair = [a.key, b.key];
        if a.offset + a.len != b.offset || !(pair.contains(&own) && pair.contains(&other)) {
            continue;
        }
        let at = usize::try_from(a.offset).expect("offset");
        assert_eq!(&t.stream[at - 2..at], &[REF_VIEWS, 2], "a Ref::views of two keys");
        assert!(a.key < b.key, "Ref::views writes its keys ascending");
        seen += 1;
    }
    assert_eq!(seen, 1, "one Ref::views in f: {:?}", t.refs);
}

// Two observers that include the parts in either order write the same
// unit of f (the own key does not follow the include order).
#[test]
fn views_do_not_depend_on_the_include_order() {
    let a = with_post("dep.h", "own.h", true, slip_digest::digest);
    let b = with_post("own.h", "dep.h", true, slip_digest::digest);
    assert_eq!(units_of_f(&a), units_of_f(&b));
}
