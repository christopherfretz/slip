//! The tables test (`spec/digest-kinds.md` §1 and "Tests"; `spec/digest.md`
//! §13 obligations 3, 4 and 5; WP-023): `spec/digest-kinds.md` parsed by
//! the grammar of its §1, compared with Kiln's schema descriptors, the
//! generated `src/tables.rs` kept current, and the rows held to their
//! tests with the ratchet `tests/tables/untested.txt`.
//!
//! `SLIP_BLESS=1` rewrites `src/tables.rs`, `tests/tables/extra.rs`, the
//! hashes of `tests/tables/pins.txt`, and removes from `untested.txt` the
//! names whose test now exists.

#[path = "tables/mod.rs"]
mod tables;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

use slip_kiln::kiln::ast::Ast;
use slip_kiln::kiln::snap::{Schema, Snap};
use tables::kiln::Descs;
use tables::{Doc, check, generate, parse};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn doc() -> &'static Doc {
    static DOC: OnceLock<Doc> = OnceLock::new();
    DOC.get_or_init(|| parse(&read(&manifest().join("../../spec/digest-kinds.md"))))
}

/// The descriptors of `Ast` and of the `pin=extra` registrations.
fn descs() -> &'static Descs {
    static DESCS: OnceLock<Descs> = OnceLock::new();
    DESCS.get_or_init(|| {
        let mut ast = Schema::new();
        <Ast as Snap>::schema(&mut ast);
        let mut extra = Schema::new();
        tables::extra::register(&mut extra);
        Descs::parse(&ast.hashes(), &extra.hashes()).unwrap_or_else(|e| panic!("{e}"))
    })
}

fn blessing() -> bool {
    std::env::var_os("SLIP_BLESS").is_some()
}

fn fail_if(what: &str, errors: &[String]) {
    assert!(errors.is_empty(), "{what}: {} problem(s)\n{}", errors.len(), errors.join("\n"));
}

/// Compares a generated file with what is checked in, or writes it when
/// blessing.
fn current(rel: &str, actual: &str) {
    let path = manifest().join(rel);
    if blessing() {
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .map_or_else(|| expected.lines().count().min(actual.lines().count()) + 1, |k| k + 1);
        panic!(
            "crates/slip_digest/{rel} is not what the generator gives (first difference at line \
             {line}); run `SLIP_BLESS=1 cargo test -p slip_digest --test tables` and review the diff"
        );
    }
}

fn pins_text() -> String {
    read(&manifest().join("tests/tables/pins.txt"))
}

fn unnameable() -> BTreeSet<String> {
    generate::pins_names(&pins_text()).unwrap_or_else(|e| panic!("{}", e.join("\n")))
}

/// Every `<root>/<short>/<row>` under `tests/` that holds a file (§1.5),
/// but `<root>/_/`, which holds the tests no row names.
fn test_dirs(base: &Path) -> BTreeSet<(String, String)> {
    fn has_file(p: &Path) -> bool {
        std::fs::read_dir(p).into_iter().flatten().flatten().any(|e| {
            let p = e.path();
            if p.is_dir() { has_file(&p) } else { true }
        })
    }
    let mut out = BTreeSet::new();
    for root in check::ROOTS {
        for short in std::fs::read_dir(base.join(root)).into_iter().flatten().flatten() {
            // `_/<slug>/`: a test no row names (WP-024).
            if short.file_name() == "_" {
                continue;
            }
            for row in std::fs::read_dir(short.path()).into_iter().flatten().flatten() {
                if row.path().is_dir() && has_file(&row.path()) {
                    let name = format!(
                        "{}/{}",
                        short.file_name().to_string_lossy(),
                        row.file_name().to_string_lossy()
                    );
                    out.insert(((*root).to_owned(), name));
                }
            }
        }
    }
    out
}

// --- The checks on the real file -------------------------------------------

/// §1.1 to §1.4.
#[test]
fn grammar() {
    fail_if("digest-kinds.md against its grammar (§1)", &check::grammar(doc()));
}

/// Digest §13 obligation 4: both ways, and every `pin`.
#[test]
fn against_kiln() {
    fail_if(
        "digest-kinds.md against Kiln's descriptors (§13 obligation 4)",
        &check::against_kiln(doc(), descs()),
    );
}

/// The generated file is what the generator gives for the file and the
/// linked Kiln (obligation 3 lives in it).
#[test]
fn generated_file_is_current() {
    let out = generate::tables_rs(doc(), descs(), &unnameable())
        .unwrap_or_else(|e| panic!("{}", e.join("\n")));
    current("src/tables.rs", &out);
}

/// The feature `unpinned` leaves the compile-time pins out so that this
/// test can bless them after the Kiln pin moved; any other run under it
/// would pass without obligation 3.
#[test]
fn unpinned_only_when_blessing() {
    assert!(
        !cfg!(feature = "unpinned") || blessing(),
        "the feature `unpinned` is only for `SLIP_BLESS=1 cargo test -p slip_digest --test \
         tables --features unpinned`"
    );
}

/// The registrations of the `pin=extra` types follow the file.
#[test]
fn extra_registrations_are_current() {
    current("tests/tables/extra.rs", &generate::extra_rs(doc(), &unnameable()));
}

/// Obligation 3 for the types Slip cannot name: `pins.txt`.
#[test]
fn recorded_pins_are_current() {
    let out = generate::pins_txt(doc(), descs(), &unnameable())
        .unwrap_or_else(|e| panic!("{}", e.join("\n")));
    if !blessing() && out != pins_text() {
        let have = pins_text();
        let diff: Vec<_> = out.lines().filter(|l| !have.lines().any(|h| h == *l)).collect();
        panic!(
            "tests/tables/pins.txt: these pins changed (review the rows of the type, then \
             `SLIP_BLESS=1 cargo test -p slip_digest --test tables`):\n{}",
            diff.join("\n")
        );
    }
    current("tests/tables/pins.txt", &out);
}

/// Digest §13 obligation 5 with the ratchet of WP-023.
#[test]
fn rows_against_tests() {
    let path = manifest().join("tests/tables/untested.txt");
    let text = read(&path);
    let dirs = test_dirs(&manifest().join("tests"));
    if blessing() {
        let out = check::bless_untested(doc(), &text, &dirs);
        if out != text {
            std::fs::write(&path, &out).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            eprintln!("blessed {}", path.display());
        }
        return;
    }
    fail_if("rows against tests (§13 obligation 5)", &check::ratchet(doc(), &text, &dirs));
}

/// The test takes well under ten seconds once built: the whole file and
/// Kiln's descriptors are read once.
#[test]
fn summary() {
    let d = doc();
    let rows: usize = d.types.iter().map(|t| t.rows.len()).sum();
    let pins = generate::pinned(d, descs());
    let untested = check::untested(&read(&manifest().join("tests/tables/untested.txt"))).0.len();
    eprintln!(
        "digest-kinds.md: {} types, {rows} rows; {} pinned instantiations ({} in pins.txt); {untested} names in untested.txt",
        d.types.len(),
        pins.len(),
        unnameable().len()
    );
    assert!(rows > 0 && !pins.is_empty());
}

// --- Each check on small tables ---------------------------------------------

mod unit {
    use std::collections::BTreeSet;

    use super::tables::kiln::Descs;
    use super::tables::{Base, Encoding, Suffix, check, generate, parse, parse_encoding, snake};

    /// Slip's own tables (§3.1, §3.2, §11.1) and the list of unpinned
    /// types, which every file needs.
    const SLIP: &str = "\
### 3.1 KeyKind

| Tag | KeyKind | Entities |
|---|---|---|
| 1 | `namespace` | a namespace |
| 2 | `class_template` | |

### 3.2 Name

| Tag | Name | Then |
|---|---|---|
| 1 | `identifier` | |

### 11.1 Resolved properties

| Number | Property | Belongs to | Stream |
|---|---|---|---|
| 1 | `exception_spec` | | |
| 2 | retired | | |

### Types nothing pins

`V` has no descriptor.
";

    const TYPES: &str = "\
### `kiln_x::E`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `A` | flag | in | tag 1 |  |
| `B` | written | out | as A | spelling of `A` |
| `CxxRecord` | flag | in | tag 3 |  |
| `CxxRecord.0` | value | in | uv |  |
| `D` | flag | unsupported | unbuilt | no construction site |

### `kiln_x::S`

`kind=struct; pin=schema; selector=kiln_x::E`

Prose between the property line and the table.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum E | the tag |
| `w-low` | value | in | uv |  |
| `w-rest` | bookkeeping | out | - |  |
| `A.v` | child | in | child*? |  |
| `loc` | location | out | - |  |

### `kiln_x::X`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `P` | flag | demand | - |  |
| `Q` | flag | in | tag 2 |  |

### `kiln_x::V`

`kind=view; pin=none; base=1024`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `One` | type | in | tag 1025 |  |
| `One.ty` | type | in | type |  |
";

    const AST: &[(&str, Option<u64>)] = &[
        ("kiln_x::S = struct kind:kiln_x::E w:u32 v:alloc::vec::Vec<(u32, u32)> loc:u32", Some(1)),
        ("kiln_x::E = enum | A=0 | B | CxxRecord 0:u32 | D", Some(2)),
        ("u32 = int", Some(3)),
        ("alloc::vec::Vec<(u32, u32)> = seq 0:(u32, u32)", Some(4)),
        ("(u32, u32) = tuple 0:u32 1:u32", Some(5)),
    ];
    const EXTRA: &[(&str, Option<u64>)] = &[("kiln_x::X = enum | P | Q", Some(0xabcd))];

    fn file(types: &str) -> String {
        format!("{SLIP}\n{types}")
    }

    fn descs(ast: &[(&str, Option<u64>)]) -> Descs {
        Descs::parse(ast, EXTRA).unwrap()
    }

    fn grammar(types: &str) -> Vec<String> {
        check::grammar(&parse(&file(types)))
    }

    fn kiln(types: &str, ast: &[(&str, Option<u64>)]) -> Vec<String> {
        let doc = parse(&file(types));
        let mut e = check::grammar(&doc);
        e.extend(check::against_kiln(&doc, &descs(ast)));
        e
    }

    #[track_caller]
    fn has(errors: &[String], what: &str) {
        assert!(errors.iter().any(|e| e.contains(what)), "no error with `{what}` in {errors:#?}");
    }

    fn edit(from: &str, to: &str) -> String {
        assert!(TYPES.contains(from), "{from}");
        TYPES.replacen(from, to, 1)
    }

    #[test]
    fn s1_the_small_file_is_clean() {
        assert_eq!(kiln(TYPES, AST), Vec::<String>::new());
        let doc = parse(&file(TYPES));
        assert_eq!(doc.types.len(), 4);
        let s = &doc.types[1];
        assert_eq!((s.short.as_str(), s.rows.len()), ("S", 5));
        let r = &s.rows[3];
        assert_eq!(
            (r.kind.as_deref(), r.member.as_str(), r.fact.as_deref()),
            (Some("A"), "v", None)
        );
        assert_eq!(r.encoding, Encoding::Form(Base::Child, vec![Suffix::List, Suffix::Opt]));
        assert_eq!(s.rows[1].fact.as_deref(), Some("low"));
        assert_eq!(doc.types[0].number_of("B"), Some(2));
        assert_eq!(doc.types[3].number_of("One"), Some(1025));
        assert_eq!(doc.properties.len(), 2);
        assert!(doc.properties[1].name.is_none());
    }

    /// §1.7: the walk's rows are numbered from `WALK_BASE + 1` in order,
    /// outside every enumeration's range.
    #[test]
    fn s1_7_the_walks_rows() {
        let walk = |rows: &str| {
            format!("### 1.7 Rows of the walk\n\n| Tag | Row | Written |\n|---|---|---|\n{rows}\n")
        };
        let doc = parse(&format!("{SLIP}\n{}\n{TYPES}", walk("| 1537 | `backref` | |")));
        assert_eq!(doc.walk.len(), 1);
        assert_eq!(check::grammar(&doc), Vec::<String>::new());
        let doc = parse(&format!("{SLIP}\n{}\n{TYPES}", walk("| 1538 | `backref` | |")));
        has(&check::grammar(&doc), "out of order: 1537 expected");
        let doc =
            parse(&format!("{SLIP}\n{}\n{TYPES}", walk("| 1537 | `a` | |\n| 1538 | `a` | |")));
        has(&check::grammar(&doc), "listed twice");
        // A table of variants whose range reaches the walk's.
        let types = edit("`kind=view; pin=none; base=1024`", "`kind=view; pin=none; base=1536`")
            .replace("tag 1025", "tag 1537");
        let doc = parse(&format!("{SLIP}\n{}\n{types}", walk("| 1537 | `backref` | |")));
        has(&check::grammar(&doc), "overlap");
    }

    /// §1.3: the forms and their suffixes.
    #[test]
    fn s1_3_encodings() {
        assert_eq!(
            parse_encoding("ref*sorted"),
            Ok(Encoding::Form(Base::Ref, vec![Suffix::Sorted]))
        );
        assert_eq!(
            parse_encoding("enum ArgView*?"),
            Ok(Encoding::Form(Base::Enum("ArgView".into()), vec![Suffix::List, Suffix::Opt]))
        );
        assert_eq!(parse_encoding("tag 7"), Ok(Encoding::Tag(7)));
        assert_eq!(parse_encoding("as Nothrow"), Ok(Encoding::As("Nothrow".into())));
        assert!(parse_encoding("uv!").is_err());
        assert!(parse_encoding("tag 07").is_err());
        assert!(parse_encoding("bit?").is_err());
        assert!(parse_encoding("words").is_err());
    }

    /// §1.2: a line that looks like a row and does not parse is an error.
    #[test]
    fn s1_2_bad_rows_are_errors() {
        has(
            &grammar(&edit("| `w-rest` | bookkeeping |", "| w-rest | bookkeeping |")),
            "is not one code span",
        );
        has(
            &grammar(&edit("| `loc` | location | out | - |  |", "| `loc` | location | out | - |")),
            "5 cells",
        );
        has(&grammar(&edit("| `loc` | location |", "| `loc` | place |")), "unknown class `place`");
        has(
            &grammar(&edit("| `loc` | location | out |", "| `loc` | location | maybe |")),
            "unknown decision",
        );
        has(&grammar(&edit("`w-low`", "`w-Low`")), "lower-case words");
        has(
            &grammar(&edit(
                "`kind=struct; pin=schema; selector",
                "`kind=struct; pin=sometimes; selector",
            )),
            "unknown pin",
        );
        has(&grammar(&edit("### `kiln_x::V`", "#### `kiln_x::V`")), "level 3");
    }

    #[test]
    fn s1_2_field_row_that_does_not_exist() {
        has(
            &kiln(&edit("| `loc` |", "| `gone` | location | out | - |  |\n| `loc` |"), AST),
            "`gone` is not a field of `kiln_x::S`",
        );
    }

    #[test]
    fn s1_2_field_without_row() {
        let mut ast = AST.to_vec();
        ast[0] = (
            "kiln_x::S = struct kind:kiln_x::E w:u32 v:alloc::vec::Vec<(u32, u32)> loc:u32 more:u32",
            Some(1),
        );
        has(&kiln(TYPES, &ast), "kiln_x::S: field `more` has no row");
        let mut ast = AST.to_vec();
        ast[1] = ("kiln_x::E = enum | A=0 | B | CxxRecord 0:u32 1:u32 | D | F", Some(2));
        let e = kiln(TYPES, &ast);
        has(&e, "kiln_x::E: field `CxxRecord.1` has no row");
        has(&e, "kiln_x::E: variant `F` has no row");
    }

    #[test]
    fn s1_2_kind_prefix_not_a_variant() {
        let e = kiln(&edit("`A.v`", "`Z.v`"), AST);
        has(&e, "`Z` is not a variant of `E`");
        has(&e, "`Z` is not a variant of `kiln_x::E`");
    }

    #[test]
    fn s1_1_duplicate_short_name() {
        let types = format!(
            "{TYPES}\n### `kiln_y::E`\n\n`kind=enum; pin=none`\n\n| Row | Class | Decision | Encoding | Note |\n|---|---|---|---|---|\n| `A` | flag | in | tag 1 |  |\n"
        );
        has(&grammar(&types), "short name `E` is also the type of line");
    }

    #[test]
    fn s1_4_tag_out_of_step() {
        has(&grammar(&edit("| tag 3 |", "| tag 4 |")), "tag 4, but §1.4 gives 3");
        // A retired row keeps its number: the rows after it do not move.
        let retired = edit(
            "| `CxxRecord` |",
            "| `Old` | flag | unsupported | unbuilt | retired |\n| `CxxRecord` |",
        );
        has(&grammar(&retired), "tag 3, but §1.4 gives 4");
        has(
            &grammar(&edit("| `One` | type | in | tag 1025 |", "| `One` | type | in | tag 1 |")),
            "gives 1025",
        );
    }

    #[test]
    fn s1_4_base_ranges_disjoint() {
        let types = format!(
            "{TYPES}\n### `kiln_x::W`\n\n`kind=view; pin=none; base=1023`\n\n| Row | Class | Decision | Encoding | Note |\n|---|---|---|---|---|\n| `A` | flag | out | - | x |\n| `B` | flag | in | tag 1025 |  |\n"
        );
        has(&grammar(&types), "overlap");
    }

    #[test]
    fn s1_4_as_names_no_variant() {
        has(&grammar(&edit("| as A |", "| as Nope |")), "`as Nope` names no other variant of `E`");
        has(
            &grammar(&edit(
                "| `w-rest` | bookkeeping | out | - |",
                "| `w-rest` | bookkeeping | out | as A |",
            )),
            "cannot have as A",
        );
    }

    #[test]
    fn s1_3_enum_names_no_table() {
        has(&grammar(&edit("| enum E |", "| enum Missing |")), "`Missing` names no tabled type");
        has(&grammar(&edit("| enum E |", "| record E |")), "`record E` names an enumeration");
        has(&grammar(&edit("| enum E |", "| enum S |")), "names a table without variants");
    }

    #[test]
    fn s1_2_in_row_without_encoding() {
        has(
            &grammar(&edit("| `w-low` | value | in | uv |", "| `w-low` | value | in | - |")),
            "an `in` row needs an encoding",
        );
        has(
            &grammar(&edit("| `A` | flag | in | tag 1 |", "| `A` | flag | in | uv |")),
            "an `in` variant row is `tag N`",
        );
        has(
            &grammar(&edit("| `w-low` | value | in | uv |", "| `w-low` | value | in | tag 2 |")),
            "`tag N` is for a variant row",
        );
    }

    #[test]
    fn s1_2_out_row_with_encoding() {
        has(
            &grammar(&edit("| `loc` | location | out | - |", "| `loc` | location | out | uv |")),
            "a `out` row cannot have an encoding",
        );
        has(
            &grammar(&edit(
                "| `loc` | location | out | - |",
                "| `loc` | location | out | unbuilt |",
            )),
            "cannot have unbuilt",
        );
        has(&grammar(&edit("| spelling of `A` |", "|  |")), "gives its reason in Note");
    }

    #[test]
    fn s1_4_retired_row_names_nothing() {
        let d = "| `D` | flag | unsupported | unbuilt | no construction site |";
        let retired = edit(d, &format!("{d}\n| `Old` | flag | unsupported | unbuilt | retired |"));
        let e = kiln(&retired, AST);
        assert!(!e.iter().any(|x| x.contains("Old")), "{e:#?}");
        // A row that is not retired must name a variant.
        has(
            &kiln(&edit(d, &format!("{d}\n| `Old` | flag | unsupported | unbuilt | gone |")), AST),
            "`Old` is not a variant",
        );
    }

    #[test]
    fn s1_2_fact_row_and_own_row() {
        has(
            &grammar(&edit("| `w-low` |", "| `w` | value | in | uv |  |\n| `w-low` |")),
            "`w` has a row of its own and fact rows",
        );
    }

    #[test]
    fn s1_1_wrong_pin() {
        has(
            &kiln(&edit("`kind=enum; pin=extra`", "`kind=enum; pin=schema`"), AST),
            "`pin=schema`, but it is `pin=extra`",
        );
        has(
            &kiln(&edit("`kind=view; pin=none; base", "`kind=view; pin=schema; base"), AST),
            "`pin=schema`, but `Ast`'s descriptor does not reach it",
        );
        has(
            &kiln(&edit("`kind=view; pin=none; base", "`kind=view; pin=extra; base"), AST),
            "`pin=extra`, but registering it gives no descriptor",
        );
        has(
            &kiln(&edit("`kind=enum; pin=schema`", "`kind=enum; pin=extra`"), AST),
            "`pin=extra`, but it is `pin=schema`",
        );
        has(
            &kiln(&edit("`kind=enum; pin=extra`", "`kind=enum; pin=none`"), AST),
            "`pin=none`, but it is `pin=extra`",
        );
        let unlisted = file(TYPES).replace("`V` has no descriptor.", "");
        has(
            &check::against_kiln(&parse(&unlisted), &descs(AST)),
            "\"Types nothing pins\" does not list it",
        );
    }

    #[test]
    fn s1_1_descriptor_kinds() {
        let mut ast = AST.to_vec();
        ast.push(("kiln_x::T = struct a:u32", Some(9)));
        ast.push(("kiln_x::Odd = odd a:u32", Some(9)));
        let e = kiln(TYPES, &ast);
        has(&e, "kiln_x::T: a `struct` descriptor with no heading");
        has(&e, "kiln_x::Odd: descriptor kind `odd` is neither tabled nor listed");
        let mut ast = AST.to_vec();
        ast[0] = ("kiln_x::S = seq 0:u32", Some(1));
        has(&kiln(TYPES, &ast), "a descriptor of kind `seq` has no table");
    }

    /// The descriptor text, notes and generic arguments included.
    #[test]
    fn descriptors() {
        let d = Descs::parse(
            &[(
                "kiln_x::M<kiln_x::K, (u32, u16)> = root #M: name as a string, then the map name:alloc::string::String map:std::collections::HashMap<kiln_x::K, (u32, u16)> s:skip:x::Y",
                Some(1),
            )],
            &[],
        )
        .unwrap();
        let m = d.of("kiln_x::M").next().unwrap();
        assert_eq!(m.fields, ["name", "map", "s"]);
        assert!(Descs::parse(&[("kiln_x::Q = struct what", None)], &[]).is_err());
    }

    /// §1.5 and the ratchet of WP-023.
    #[test]
    fn s1_5_ratchet() {
        let doc = parse(&file(TYPES));
        let names: Vec<_> =
            check::obligations(&doc).into_iter().map(|(n, (r, _))| format!("{r} {n}")).collect();
        assert_eq!(
            names,
            [
                "differ E/A",
                "equal E/B",
                "differ E/CxxRecord",
                "differ E/CxxRecord.0",
                "differ S/A.v",
                "differ S/kind",
                "differ S/w-low",
                "differ V/One",
                "differ V/One.ty",
                "demand X/P",
                "differ X/Q",
            ]
        );
        let all = "# a comment\nE/A\nE/B\nE/CxxRecord\nE/CxxRecord.0\nS/A.v\nS/kind\nS/w-low\nV/One\nV/One.ty\nX/P\nX/Q\n";
        let none = BTreeSet::new();
        assert_eq!(check::ratchet(&doc, all, &none), Vec::<String>::new());
        has(
            &check::ratchet(&doc, &all.replace("E/B\n", ""), &none),
            "`E/B` has no test: `tests/equal/E/B/` is missing",
        );
        let one: BTreeSet<_> = [("equal".to_owned(), "E/B".to_owned())].into();
        has(&check::ratchet(&doc, all, &one), "untested.txt:3: `E/B` has its test");
        assert_eq!(check::ratchet(&doc, &all.replace("E/B\n", ""), &one), Vec::<String>::new());
        assert_eq!(check::bless_untested(&doc, all, &one), all.replace("E/B\n", ""));
        let wrong: BTreeSet<_> = [("differ".to_owned(), "E/B".to_owned())].into();
        has(&check::ratchet(&doc, all, &wrong), "tests/differ/E/B/: no row names this test");
        has(&check::ratchet(&doc, &format!("{all}X/R\n"), &none), "`X/R` is no row's test name");
        has(&check::ratchet(&doc, &all.replace("E/A\nE/B\n", "E/B\nE/A\n"), &none), "out of order");
        has(&check::ratchet(&doc, &format!("{all}# late\n"), &none), "at the top only");
        // Unbuilt and bookkeeping rows oblige nothing.
        has(
            &check::ratchet(&doc, &all.replace("E/CxxRecord\n", "E/CxxRecord\nE/D\n"), &none),
            "`E/D` is no row's test name",
        );
    }

    #[test]
    fn snake_case() {
        assert_eq!(snake("DeclKind"), "decl_kind");
        assert_eq!(snake("CXXRecord"), "cxx_record");
        assert_eq!(snake("RValueRef"), "r_value_ref");
        assert_eq!(snake("X87Extended"), "x87_extended");
        assert_eq!(super::tables::screaming("ALIGNED"), "ALIGNED");
        assert_eq!(super::tables::screaming("class_template"), "CLASS_TEMPLATE");
        assert_eq!(
            generate::rust_type("alloc::boxed::Box<[kiln_ast::types::nns::Nns]>"),
            "Box<[kiln::ast::types::Nns]>"
        );
        // `types::node` is public since Kiln `83566cc47`: no re-export.
        assert_eq!(
            generate::rust_type("kiln_ast::types::node::Store"),
            "kiln::ast::types::node::Store"
        );
        assert_eq!(generate::rust_type("(kiln_source::loc::Loc, u32)"), "(kiln::source::Loc, u32)");
    }

    /// The generator's output for the small file, as a golden.
    #[test]
    fn generator_golden() {
        let doc = parse(&file(TYPES));
        let none = BTreeSet::new();
        let out = generate::tables_rs(&doc, &descs(AST), &none).unwrap();
        let extra = generate::extra_rs(&doc, &none);
        let unnameable: BTreeSet<_> = ["kiln_x::S".to_owned()].into();
        let pins = generate::pins_txt(&doc, &descs(AST), &unnameable).unwrap();
        let hidden = generate::tables_rs(&doc, &descs(AST), &unnameable).unwrap();
        assert!(hidden.contains("// `kiln_x::S` (schema): in `tests/tables/pins.txt`."));
        assert!(
            generate::pins_txt(&doc, &descs(AST), &["kiln_x::Nope".to_owned()].into()).is_err()
        );
        let actual = format!("{out}\n---- extra.rs\n{extra}\n---- pins.txt\n{pins}");
        golden("tables_small.txt", &actual);
    }

    fn golden(name: &str, actual: &str) {
        let path = super::manifest().join("tests/golden").join(name);
        if super::blessing() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, actual).unwrap();
            eprintln!("blessed {}", path.display());
            return;
        }
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
        assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
    }
}
