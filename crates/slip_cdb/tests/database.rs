//! `spec/cdb.md` §2: reading a JSON compilation database. Well-formed
//! entries in both forms, every defect (each with the entry still numbered
//! and its neighbours unaffected), and the three `DatabaseError` cases.

use slip_cdb::database::{Defect, Entry, read_json};

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// A well-formed entry used as a neighbour of every defect.
const GOOD: &str = r#"{"directory": "/b", "file": "a.cc", "arguments": ["cc", "-c", "a.cc"]}"#;

fn good() -> Entry {
    Entry { directory: "/b".into(), file: "a.cc".into(), arguments: strings(&["cc", "-c", "a.cc"]) }
}

/// Reads `[GOOD, element, GOOD]` and returns the middle result, checking
/// that the neighbours are unaffected and the numbering kept.
fn middle(element: &str) -> Result<Entry, Defect> {
    let text = format!("[{GOOD}, {element}, {GOOD}]");
    let mut entries = read_json(text.as_bytes()).expect("database reads");
    assert_eq!(entries.len(), 3, "one result per entry");
    assert_eq!(entries[0], Ok(good()), "entry 0 unaffected");
    assert_eq!(entries[2], Ok(good()), "entry 2 unaffected");
    entries.swap_remove(1)
}

fn defect(element: &str) -> Defect {
    match middle(element) {
        Ok(entry) => panic!("{element} was well formed: {entry:?}"),
        Err(d) => d,
    }
}

/// Asserts a defect whose detail mentions `needle`, with `file` as given.
fn assert_defect(element: &str, file: &str, needle: &str) {
    let d = defect(element);
    assert_eq!(d.file, file, "{element}: file as spelled");
    assert!(d.detail.contains(needle), "{element}: detail {:?} lacks {needle:?}", d.detail);
}

// ---------------------------------------------------------------------------
// Well-formed entries.
// ---------------------------------------------------------------------------

#[test]
fn s2_arguments_form() {
    let e =
        middle(r#"{"directory": "/w", "file": "/s/x.c", "arguments": ["gcc", "-DA B", "x.c"]}"#)
            .unwrap();
    assert_eq!(
        e,
        Entry {
            directory: "/w".into(),
            file: "/s/x.c".into(),
            arguments: strings(&["gcc", "-DA B", "x.c"]),
        }
    );
}

#[test]
fn s2_command_form_is_split_by_s3() {
    let e = middle(r#"{"directory": "/w", "file": "x.c", "command": "gcc -D\"A B\"'C' -c x.c"}"#)
        .unwrap();
    assert_eq!(e.arguments, strings(&["gcc", "-DA BC", "-c", "x.c"]));
}

#[test]
fn s2_both_present_arguments_wins() {
    let e = middle(
        r#"{"directory": "/w", "file": "x.c", "arguments": ["clang", "x.c"], "command": "gcc y.c"}"#,
    )
    .unwrap();
    assert_eq!(e.arguments, strings(&["clang", "x.c"]));
}

#[test]
fn s2_both_present_arguments_wins_even_over_an_unsplittable_command() {
    let e = middle(
        r#"{"directory": "/w", "file": "x.c", "arguments": ["clang", "x.c"], "command": "\""}"#,
    )
    .unwrap();
    assert_eq!(e.arguments, strings(&["clang", "x.c"]));
}

#[test]
fn s2_other_keys_and_output_ignored() {
    let e = middle(
        r#"{"output": "x.o", "directory": "/w", "zzz": [1, {"a": null}], "file": "x.c",
            "arguments": ["cc", "x.c"], "extra": true}"#,
    )
    .unwrap();
    assert_eq!(
        e,
        Entry { directory: "/w".into(), file: "x.c".into(), arguments: strings(&["cc", "x.c"]) }
    );
}

#[test]
fn s2_empty_array_is_no_entries() {
    assert_eq!(read_json(b"[]").unwrap(), Vec::new());
}

#[test]
fn s2_entries_numbered_in_order() {
    let text = format!(
        "[{GOOD}, 1, {}]",
        r#"{"directory": "/c", "file": "z.cc", "arguments": ["cc", "z.cc"]}"#
    );
    let entries = read_json(text.as_bytes()).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], Ok(good()));
    assert!(entries[1].is_err());
    assert_eq!(entries[2].as_ref().unwrap().directory, "/c");
}

// ---------------------------------------------------------------------------
// Defects: each is malformed_entry with the entry still numbered.
// ---------------------------------------------------------------------------

#[test]
fn s2_defect_not_an_object() {
    for element in ["1", "\"x\"", "null", "true", "[]", "1.5"] {
        assert_defect(element, "", "not an object");
    }
}

#[test]
fn s2_defect_directory_missing() {
    assert_defect(
        r#"{"file": "x.c", "arguments": ["cc", "x.c"]}"#,
        "x.c",
        "`directory` is missing",
    );
}

#[test]
fn s2_defect_directory_not_a_string() {
    for value in ["1", "null", "[\"/w\"]", "{}", "false"] {
        let element =
            format!(r#"{{"directory": {value}, "file": "x.c", "arguments": ["cc", "x.c"]}}"#);
        assert_defect(&element, "x.c", "`directory` is");
        assert!(defect(&element).detail.contains("not a string"));
    }
}

#[test]
fn s2_defect_directory_relative() {
    for value in ["\"w\"", "\"./w\"", "\"\"", "\"../w\""] {
        let element =
            format!(r#"{{"directory": {value}, "file": "x.c", "arguments": ["cc", "x.c"]}}"#);
        assert_defect(&element, "x.c", "not an absolute path");
    }
}

#[test]
fn s2_defect_file_missing() {
    assert_defect(r#"{"directory": "/w", "arguments": ["cc", "x.c"]}"#, "", "`file` is missing");
}

#[test]
fn s2_defect_file_not_a_string() {
    for value in ["1", "null", "[\"x.c\"]"] {
        let element =
            format!(r#"{{"directory": "/w", "file": {value}, "arguments": ["cc", "x.c"]}}"#);
        // Defect.file is empty when `file` is not a string.
        assert_defect(&element, "", "`file` is");
    }
}

#[test]
fn s2_defect_file_kept_when_directory_is_bad() {
    // The defect carries `file` as spelled whatever else is wrong.
    assert_defect(r#"{"directory": 3, "file": "src/y.cc"}"#, "src/y.cc", "`directory`");
}

#[test]
fn s2_defect_neither_arguments_nor_command() {
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "output": "x.o"}"#,
        "x.c",
        "neither `arguments` nor `command`",
    );
}

#[test]
fn s2_defect_empty_arguments() {
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "arguments": []}"#,
        "x.c",
        "`arguments` is empty",
    );
}

#[test]
fn s2_defect_arguments_not_an_array() {
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "arguments": "cc x.c"}"#,
        "x.c",
        "`arguments` is a string, not an array",
    );
}

#[test]
fn s2_defect_non_string_argument() {
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "arguments": ["cc", 3, "x.c"]}"#,
        "x.c",
        "`arguments[1]` is a number, not a string",
    );
}

#[test]
fn s2_defect_malformed_arguments_not_rescued_by_command() {
    // `arguments` is used when both are present, so a bad one is a defect
    // even though `command` alone would do.
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "arguments": [], "command": "cc x.c"}"#,
        "x.c",
        "`arguments` is empty",
    );
}

#[test]
fn s2_defect_command_not_a_string() {
    assert_defect(
        r#"{"directory": "/w", "file": "x.c", "command": ["cc", "x.c"]}"#,
        "x.c",
        "`command` is an array, not a string",
    );
}

#[test]
fn s2_defect_unsplittable_command() {
    // §3 errors: unclosed quotes, trailing backslash, no words.
    for command in [r#""cc \"x.c""#, r#""cc 'x.c""#, r#""cc x.c\\""#, r#""""#, r#"" \t ""#] {
        let element = format!(r#"{{"directory": "/w", "file": "x.c", "command": {command}}}"#);
        assert_defect(&element, "x.c", "`command` cannot be split");
    }
}

// ---------------------------------------------------------------------------
// DatabaseError: the whole database is refused.
// ---------------------------------------------------------------------------

#[test]
fn s2_database_error_not_json() {
    for text in [&b"[{"[..], b"", b"[1,]", b"// x\n[]", b"[] []"] {
        let e = read_json(text).unwrap_err();
        assert!(e.detail.starts_with("not JSON"), "{text:?}: {}", e.detail);
    }
}

#[test]
fn s2_database_error_not_utf8() {
    let e = read_json(b"[\"\xff\"]").unwrap_err();
    assert!(e.detail.starts_with("not JSON"), "{}", e.detail);
}

#[test]
fn s2_database_error_top_level_not_an_array() {
    for (text, kind) in [
        ("{}", "an object"),
        (r#"{"entries": []}"#, "an object"),
        ("3", "a number"),
        ("\"[]\"", "a string"),
        ("null", "null"),
    ] {
        let e = read_json(text.as_bytes()).unwrap_err();
        assert_eq!(e.detail, format!("the top level is {kind}, not an array"), "{text}");
    }
}
