//! `E(k)` does not depend on the order the walk meets a namespace's
//! members (WP-141's ruling on the one predicate: a view exists iff a
//! member of that ownership exists, decided from the declarations of the
//! image, never from what the walk keyed so far; `Walk::ns_holds`).
//!
//! The post-image of `relocate legacy => :: acme`, where a dependency's
//! `legacy` never migrates and a dependency opens `acme`: the owned `acme` (keyed `legacy` in its owned
//! view) and `using namespace legacy;`, which names only the dependency's
//! part of the key `legacy`. Written in two orders, the member's opening
//! before the directive and after it: the directive's unit is the same,
//! and in both it is the part the rule leaves.

use std::collections::BTreeMap;

use slip_digest::{Detail, Input, Options, Output, ReadOnly, ReadOnlyFiles};
use slip_kiln::kiln::source::FileId;
use slip_model::Digest;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};
use slip_model::transform::Ownership;

/// The dependency's `legacy`, read-only, which never migrates, and its
/// `acme`, which the owned part moves into (so `acme` has two view keys).
const DEP: &str = "namespace legacy { struct Old {}; }\nnamespace acme { struct Dep {}; }\n";
/// The same without the dependency's `acme` (the original form): no
/// namespace has two view keys, and the key `legacy` is held by two
/// namespace entities (Addendum 3).
const DEP_NO_ACME: &str = "namespace legacy { struct Old {}; }\n";
/// The owned part moved into `acme`.
const MEMBER: &str = "namespace acme { int g(int); }\n";
/// The directive that names `legacy`.
const DIRECTIVE: &str = "using namespace legacy;\n";

const ROOT: &str = "/case";

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

/// Digests `main.cpp` = `#include "dep.h"` then `body`, `dep.h`
/// read-only, under the rule.
fn digest(dep: &str, body: &str) -> Output {
    let main = format!("#include \"dep.h\"\n{body}");
    let files = [("main.cpp", main.as_str()), ("dep.h", dep)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    let extra = slip_kiln::snippet::Extra::default();
    let table = table();
    slip_kiln::snippet::parse_with_tables(&snippet, &extra, |p| {
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
        slip_digest::digest(&input, &options)
    })
    .unwrap_or_else(|e| panic!("does not parse: {e}"))
}

/// The `(name, digest)` of every unit whose name says it is a
/// using-directive.
fn directives(out: &Output) -> Vec<(String, Digest)> {
    let names: BTreeMap<Digest, String> = out.names.iter().cloned().collect();
    let mut v: Vec<(String, Digest)> = out
        .digests
        .groups
        .iter()
        .flat_map(|g| g.units.iter().flatten())
        .filter_map(|u| {
            let n = names.get(&u.key)?;
            n.contains("using_directive").then(|| (n.clone(), u.digest))
        })
        .collect();
    v.sort();
    v
}

/// The directive's unit in both orders, over `dep`: the same, and the
/// part the rule leaves in both.
fn same_in_both_orders(dep: &str) {
    let member_first = digest(dep, &format!("{MEMBER}{DIRECTIVE}"));
    let directive_first = digest(dep, &format!("{DIRECTIVE}{MEMBER}"));
    let a = directives(&member_first);
    let b = directives(&directive_first);
    assert_eq!(a.len(), 1, "one directive: {a:?}");
    // Teeth: the directive is not whole for `legacy` (the owned part of
    // the key is acme's), in either order, the one that meets the
    // directive before the member included.
    for (name, _) in a.iter().chain(&b) {
        assert!(name.contains("the part the rule leaves"), "a part in both orders: {name}");
    }
    assert_eq!(a, b, "the directive's unit is the same in both orders");
}

#[test]
fn e_k_does_not_depend_on_the_order_of_members() {
    same_in_both_orders(DEP);
}

// The original form, without the dependency's `acme`: under Addendum 2's
// image-wide guard the directive was plain here (no namespace has two
// view keys), for the wrong reason; by key (Addendum 3) the key `legacy`
// is held by two namespaces, so it is the part the rule leaves.
#[test]
fn e_k_does_not_depend_on_the_order_of_members_one_view_key_each() {
    same_in_both_orders(DEP_NO_ACME);
}
