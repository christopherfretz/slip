//! Tests for `slip_model::json` (`spec/model.md` §4). Each test names the
//! sentence of §4 it covers.

use std::fmt::Write as _;

use slip_model::json::{
    Error, INT_END, INT_MIN, MAX_DEPTH, Value, WriteError, parse, write_compact, write_pretty,
};

fn ok(text: &str) -> Value {
    parse(text.as_bytes()).unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
}

fn err(bytes: &[u8]) -> Error {
    match parse(bytes) {
        Ok(v) => panic!("{:?} should be rejected, got {v:?}", String::from_utf8_lossy(bytes)),
        Err(e) => e,
    }
}

/// Asserts a rejection at `offset` whose message contains `needle`.
fn rejects(bytes: &[u8], offset: usize, needle: &str) {
    let e = err(bytes);
    assert_eq!(e.offset, offset, "offset for {:?}: {e}", String::from_utf8_lossy(bytes));
    assert!(
        e.message.contains(needle),
        "message for {:?} should contain {needle:?}: {e}",
        String::from_utf8_lossy(bytes)
    );
}

fn s(x: &str) -> Value {
    Value::Str(x.to_owned())
}

fn obj(members: &[(&str, Value)]) -> Value {
    Value::Object(members.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect())
}

// --- §4 "Value model": every kind -----------------------------------------

#[test]
fn value_model_every_kind() {
    assert_eq!(ok("null"), Value::Null);
    assert_eq!(ok("true"), Value::Bool(true));
    assert_eq!(ok("false"), Value::Bool(false));
    assert_eq!(ok("42"), Value::Int(42));
    assert_eq!(ok("-7"), Value::Int(-7));
    assert_eq!(ok("2.5"), Value::Float(2.5));
    assert_eq!(ok("\"hi\""), s("hi"));
    assert_eq!(ok("[1, \"a\", null]"), Value::Array(vec![Value::Int(1), s("a"), Value::Null]));
    assert_eq!(
        ok("{\"a\": 1, \"b\": [true]}"),
        obj(&[("a", Value::Int(1)), ("b", Value::Array(vec![Value::Bool(true)]))])
    );
    assert_eq!(ok("[]"), Value::Array(vec![]));
    assert_eq!(ok("{}"), Value::Object(vec![]));
    assert_eq!(ok(" \t\r\n 1 \t\r\n "), Value::Int(1));
}

#[test]
fn value_model_object_order_preserved_on_read_and_write() {
    let v = ok("{\"z\":1,\"a\":2,\"m\":3}");
    let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["z", "a", "m"]);
    assert_eq!(write_compact(&v).unwrap(), "{\"z\":1,\"a\":2,\"m\":3}");
}

#[test]
fn value_model_accessors() {
    let v = ok(
        "{\"n\": 5, \"neg\": -5, \"big\": 18446744073709551615, \"f\": 5.0, \"s\": \"x\", \"b\": true, \"a\": [1]}",
    );
    assert_eq!(v.get("n").and_then(Value::as_u64), Some(5));
    assert_eq!(v.get("n").and_then(Value::as_i64), Some(5));
    assert_eq!(v.get("neg").and_then(Value::as_u64), None);
    assert_eq!(v.get("neg").and_then(Value::as_i64), Some(-5));
    assert_eq!(v.get("big").and_then(Value::as_u64), Some(u64::MAX));
    assert_eq!(v.get("big").and_then(Value::as_i64), None);
    assert_eq!(v.get("f").and_then(Value::as_u64), None);
    assert_eq!(v.get("s").and_then(Value::as_str), Some("x"));
    assert_eq!(v.get("b").and_then(Value::as_bool), Some(true));
    assert_eq!(v.get("a").and_then(Value::as_array), Some(&[Value::Int(1)][..]));
    assert_eq!(v.get("missing"), None);
    assert_eq!(Value::Null.get("n"), None);
    assert_eq!(Value::Null.as_object(), None);
}

// --- §4 "A number written without fraction or exponent that fits Int is an
// Int; every other number is a Float." --------------------------------------

#[test]
fn number_classification_at_the_boundaries() {
    assert_eq!(ok("-9223372036854775808"), Value::Int(INT_MIN));
    assert_eq!(ok("18446744073709551615"), Value::Int(INT_END - 1));
    assert_eq!(ok("18446744073709551616"), Value::Float(18_446_744_073_709_551_616.0));
    assert_eq!(ok("-9223372036854775809"), Value::Float(-9_223_372_036_854_775_809.0));
    assert_eq!(ok("1.0"), Value::Float(1.0));
    assert_eq!(ok("1e3"), Value::Float(1000.0));
    assert_eq!(ok("1E+3"), Value::Float(1000.0));
    assert_eq!(ok("1e-3"), Value::Float(0.001));
    assert_eq!(ok("0"), Value::Int(0));
    // "-0" has no fraction or exponent and its value 0 fits Int.
    assert_eq!(ok("-0"), Value::Int(0));
    assert!(matches!(ok("-0.0"), Value::Float(x) if x == 0.0 && x.is_sign_negative()));
    // Beyond i128: still a Float.
    let huge = "1".repeat(60);
    assert!(matches!(ok(&huge), Value::Float(_)));
}

#[test]
fn number_non_finite_is_rejected() {
    // Landed WP-002: a number whose f64 value is infinite is rejected.
    rejects(b"[1e400]", 1, "too large");
    rejects(b"-1e400", 0, "too large");
    // Underflow is not an error.
    assert_eq!(ok("1e-400"), Value::Float(0.0));
}

// --- §4 "Accepts RFC 8259 JSON encoded as UTF-8, with an optional UTF-8
// byte-order mark." ------------------------------------------------------

#[test]
fn reader_byte_order_mark() {
    assert_eq!(parse(b"\xEF\xBB\xBF[1]").unwrap(), Value::Array(vec![Value::Int(1)]));
    assert_eq!(parse(b"\xEF\xBB\xBF \"x\" ").unwrap(), s("x"));
    // Only at the very start, and only one.
    rejects(b" \xEF\xBB\xBF1", 1, "U+FEFF");
    rejects(b"\xEF\xBB\xBF\xEF\xBB\xBF1", 3, "U+FEFF");
    // A BOM alone is not a JSON text.
    rejects(b"\xEF\xBB\xBF", 3, "end of input");
    // U+FEFF inside a string is just a character.
    assert_eq!(parse(b"\"\xEF\xBB\xBF\"").unwrap(), s("\u{feff}"));
}

#[test]
fn reader_strings_and_escapes() {
    assert_eq!(
        ok(r#""\" \\ \/ \b \f \n \r \t \u0041 \u00e9 \u00E9""#),
        s("\" \\ / \u{8} \u{c} \n \r \t A é é")
    );
    assert_eq!(ok(r#""\u0000""#), s("\0"));
    assert_eq!(ok("\"é ☃ 😀\""), s("é ☃ 😀"));
    assert_eq!(ok("\"\u{7f}\""), s("\u{7f}"));
}

#[test]
fn reader_surrogate_pairs() {
    assert_eq!(ok(r#""\ud83d\ude00""#), s("😀"));
    assert_eq!(ok(r#""\uD800\uDC00""#), s("\u{10000}"));
    assert_eq!(ok(r#""\uDBFF\uDFFF""#), s("\u{10FFFF}"));
    assert_eq!(ok(r#""a\ud83d\ude00b""#), s("a😀b"));
}

#[test]
fn reader_empty_input() {
    rejects(b"", 0, "end of input");
    rejects(b"   ", 3, "end of input");
}

// --- §4 "It must reject, with the byte offset and a message: ..." ---------
// One test per item of the list.

#[test]
fn reject_invalid_utf8() {
    rejects(b"\"ab\xFF\"", 3, "invalid UTF-8");
    rejects(b"[1, \"\xC3\"]", 5, "invalid UTF-8");
    // Overlong encoding and an encoded surrogate are invalid UTF-8 too.
    rejects(b"\"\xC0\xAF\"", 1, "invalid UTF-8");
    rejects(b"\"\xED\xA0\x80\"", 1, "invalid UTF-8");
    // Outside strings as well.
    rejects(b"1 \xFF", 2, "invalid UTF-8");
}

#[test]
fn reject_comments() {
    rejects(b"// c\n1", 0, "comments");
    rejects(b"[1 /* c */]", 3, "comments");
    rejects(b"[/* c */ 1]", 1, "comments");
    rejects(b"1 // c", 2, "comments");
    rejects(b"{\"a\": 1 # c\n}", 8, "unexpected '#'");
}

#[test]
fn reject_trailing_commas() {
    rejects(b"[1,]", 3, "trailing comma in array");
    rejects(b"[1, ]", 4, "trailing comma in array");
    rejects(b"{\"a\":1,}", 7, "trailing comma in object");
    rejects(b"[,]", 1, "unexpected ','");
    rejects(b"[1,,2]", 3, "unexpected ','");
}

#[test]
fn reject_single_quotes() {
    rejects(b"'a'", 0, "single-quoted");
    rejects(b"{'a': 1}", 1, "single-quoted");
    rejects(b"['a']", 1, "single-quoted");
}

#[test]
fn reject_leading_zeros() {
    rejects(b"01", 0, "leading zeros");
    rejects(b"[-012]", 2, "leading zeros");
    rejects(b"00.5", 0, "leading zeros");
}

#[test]
fn reject_other_number_forms() {
    rejects(b"+1", 0, "unexpected '+'");
    rejects(b".5", 0, "unexpected '.'");
    rejects(b"1.", 2, "after '.'");
    rejects(b"1.e3", 2, "after '.'");
    rejects(b"1e", 2, "exponent");
    rejects(b"1e+", 3, "exponent");
    rejects(b"-", 1, "digit");
    rejects(b"-a", 1, "digit");
    rejects(b"NaN", 0, "unexpected 'N'");
    rejects(b"Infinity", 0, "unexpected 'I'");
    rejects(b"-Infinity", 1, "digit");
    rejects(b"0x10", 1, "after the top-level value");
}

#[test]
fn reject_bare_control_characters_in_strings() {
    rejects(b"\"a\nb\"", 2, "control character U+000A");
    rejects(b"\"\t\"", 1, "control character U+0009");
    rejects(b"\"\x00\"", 1, "control character U+0000");
    rejects(b"\"\x1f\"", 1, "control character U+001F");
}

#[test]
fn reject_unpaired_surrogate_escapes() {
    rejects(br#""\ud83d""#, 1, "unpaired");
    rejects(br#""\ud83dx""#, 1, "unpaired");
    rejects(br#""ab\ud83d\n""#, 3, "unpaired");
    rejects(br#""\ud83d\u0041""#, 1, "unpaired");
    rejects(br#""\ud83d\ud83d""#, 1, "unpaired");
    rejects(br#""\ude00""#, 1, "unpaired");
    rejects(br#""\ude00\ud83d""#, 1, "unpaired");
}

#[test]
fn reject_bad_escapes() {
    rejects(br#""\x""#, 1, "invalid escape");
    rejects(br#""\'""#, 1, "invalid escape");
    rejects(br#""\u12""#, 1, "four hexadecimal digits");
    rejects(br#""\u12g4""#, 1, "four hexadecimal digits");
    rejects(br#""\ud83d\u12""#, 7, "four hexadecimal digits");
    rejects(b"\"\\", 1, "unterminated string");
    rejects(b"\"abc", 4, "unterminated string");
}

#[test]
fn reject_duplicate_keys() {
    rejects(b"{\"a\":1,\"a\":2}", 7, "duplicate key \"a\"");
    // Keys are compared after unescaping.
    rejects(b"{\"a\":1, \"\\u0061\":2}", 8, "duplicate key \"a\"");
    // Only within one object.
    assert!(parse(b"{\"a\":{\"a\":1},\"b\":{\"a\":2}}").is_ok());
    // Past the linear-scan limit too.
    let mut text = String::from("{");
    for i in 0..40 {
        write!(text, "\"k{i}\":{i},").unwrap();
    }
    let at = text.len();
    text.push_str("\"k3\":0}");
    rejects(text.as_bytes(), at, "duplicate key \"k3\"");
    text.truncate(at);
    text.push_str("\"k40\":0}");
    assert_eq!(ok(&text).as_object().unwrap().len(), 41);
}

#[test]
fn reject_nesting_deeper_than_128() {
    let deep = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(parse(deep(MAX_DEPTH).as_bytes()).is_ok());
    rejects(deep(MAX_DEPTH + 1).as_bytes(), MAX_DEPTH, "nesting deeper than 128");
    let mut objects = "{\"a\":".repeat(MAX_DEPTH - 1);
    objects.push_str("{}");
    objects.push_str(&"}".repeat(MAX_DEPTH - 1));
    assert!(parse(objects.as_bytes()).is_ok());
    let mut objects = "{\"a\":".repeat(MAX_DEPTH);
    objects.push_str("{}");
    objects.push_str(&"}".repeat(MAX_DEPTH));
    rejects(objects.as_bytes(), 5 * MAX_DEPTH, "nesting deeper than 128");
    // Far deeper input fails the same way, without exhausting the stack.
    rejects("[".repeat(1_000_000).as_bytes(), MAX_DEPTH, "nesting");
}

#[test]
fn reject_anything_after_the_top_level_value() {
    rejects(b"1 2", 2, "after the top-level value");
    rejects(b"{} {}", 3, "after the top-level value");
    rejects(b"null x", 5, "after the top-level value");
    rejects(b"[1]]", 3, "after the top-level value");
    rejects(b"1\x00", 1, "after the top-level value");
}

#[test]
fn reject_other_syntax() {
    rejects(b"tru", 0, "unexpected 't'");
    rejects(b"nul", 0, "unexpected 'n'");
    rejects(b"True", 0, "unexpected 'T'");
    rejects(b"[1", 2, "end of input");
    rejects(b"[1 2]", 3, "',' or ']'");
    rejects(b"{\"a\" 1}", 5, "':'");
    rejects(b"{\"a\":}", 5, "a value");
    rejects(b"{1:2}", 1, "a string key");
    rejects(b"{\"a\":1 \"b\":2}", 7, "',' or '}'");
    rejects(b"{", 1, "end of input");
    rejects(b"\xC2\xA01", 0, "U+00A0");
}

#[test]
fn error_line_and_column() {
    let e = err(b"{\n  \"a\": 1,\n  \"a\": 2\n}");
    assert_eq!((e.offset, e.line, e.column), (14, 3, 3));
    assert_eq!(e.to_string(), "3:3 (byte 14): duplicate key \"a\"");
    let e = err(b"[1,]");
    assert_eq!((e.line, e.column), (1, 4));
    // \r is not a line break; the BOM counts as bytes.
    let e = err(b"\xEF\xBB\xBF\r\n\r x");
    assert_eq!((e.offset, e.line, e.column), (7, 2, 3));
    let e = err(b"");
    assert_eq!((e.offset, e.line, e.column), (0, 1, 1));
}

// --- §4 "Writer" -----------------------------------------------------------

fn sample() -> Value {
    obj(&[
        ("null", Value::Null),
        ("t", Value::Bool(true)),
        ("f", Value::Bool(false)),
        ("i", Value::Int(-3)),
        ("x", Value::Float(1.5)),
        ("s", s("a/b")),
        ("ea", Value::Array(vec![])),
        ("eo", Value::Object(vec![])),
        (
            "a",
            Value::Array(vec![
                Value::Int(1),
                Value::Array(vec![Value::Int(2)]),
                obj(&[("k", Value::Null)]),
            ]),
        ),
    ])
}

#[test]
fn writer_compact_exactly() {
    assert_eq!(
        write_compact(&sample()).unwrap(),
        r#"{"null":null,"t":true,"f":false,"i":-3,"x":1.5,"s":"a/b","ea":[],"eo":{},"a":[1,[2],{"k":null}]}"#
    );
    assert_eq!(write_compact(&Value::Array(vec![])).unwrap(), "[]");
    assert_eq!(write_compact(&Value::Object(vec![])).unwrap(), "{}");
    assert_eq!(write_compact(&Value::Int(7)).unwrap(), "7");
}

#[test]
fn writer_pretty_exactly() {
    let expected = "\
{
  \"null\": null,
  \"t\": true,
  \"f\": false,
  \"i\": -3,
  \"x\": 1.5,
  \"s\": \"a/b\",
  \"ea\": [],
  \"eo\": {},
  \"a\": [
    1,
    [
      2
    ],
    {
      \"k\": null
    }
  ]
}
";
    assert_eq!(write_pretty(&sample()).unwrap(), expected);
    assert_eq!(write_pretty(&Value::Array(vec![])).unwrap(), "[]\n");
    assert_eq!(write_pretty(&Value::Object(vec![])).unwrap(), "{}\n");
    assert_eq!(write_pretty(&Value::Null).unwrap(), "null\n");
    assert_eq!(write_pretty(&s("x")).unwrap(), "\"x\"\n");
}

#[test]
fn writer_string_escapes_exactly() {
    let all_controls: String = (0u8..0x20).map(char::from).collect();
    assert_eq!(
        write_compact(&Value::Str(all_controls)).unwrap(),
        concat!(
            "\"\\u0000\\u0001\\u0002\\u0003\\u0004\\u0005\\u0006\\u0007",
            "\\b\\t\\n\\u000b\\f\\r\\u000e\\u000f",
            "\\u0010\\u0011\\u0012\\u0013\\u0014\\u0015\\u0016\\u0017",
            "\\u0018\\u0019\\u001a\\u001b\\u001c\\u001d\\u001e\\u001f\""
        )
    );
    assert_eq!(
        write_compact(&s("\"\\/\u{7f}é☃😀\u{feff}")).unwrap(),
        "\"\\\"\\\\/\u{7f}é☃😀\u{feff}\""
    );
    // Keys use the same escapes.
    assert_eq!(write_compact(&obj(&[("a\"\n", Value::Null)])).unwrap(), "{\"a\\\"\\n\":null}");
}

#[test]
fn writer_int_in_decimal() {
    assert_eq!(write_compact(&Value::Int(INT_MIN)).unwrap(), "-9223372036854775808");
    assert_eq!(write_compact(&Value::Int(INT_END - 1)).unwrap(), "18446744073709551615");
    assert_eq!(write_compact(&Value::Int(0)).unwrap(), "0");
}

#[test]
fn writer_int_out_of_range_is_an_error() {
    assert_eq!(write_compact(&Value::Int(INT_END)), Err(WriteError::IntOutOfRange(INT_END)));
    assert_eq!(
        write_pretty(&Value::Array(vec![Value::Int(INT_MIN - 1)])),
        Err(WriteError::IntOutOfRange(INT_MIN - 1))
    );
}

#[test]
fn writer_float_shortest_round_trip() {
    let cases: &[(f64, &str)] = &[
        (1.0, "1.0"),
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.5, "1.5"),
        (0.1, "0.1"),
        (1e3, "1000.0"),
        (1e15, "1000000000000000.0"),
        (1e16, "1e16"),
        (1.5e300, "1.5e300"),
        (1e-5, "1e-5"),
        (1e-4, "0.0001"),
        (f64::MAX, "1.7976931348623157e308"),
        (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
        (5e-324, "5e-324"),
        (18_446_744_073_709_551_616.0, "1.8446744073709552e19"),
    ];
    for &(x, text) in cases {
        assert_eq!(write_compact(&Value::Float(x)).unwrap(), text, "{x:e}");
        assert!(matches!(ok(text), Value::Float(y) if y.to_bits() == x.to_bits()));
    }
}

#[test]
fn writer_non_finite_float_is_an_error() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(write_compact(&Value::Float(x)), Err(WriteError::NonFiniteFloat(_))));
        assert!(matches!(
            write_pretty(&obj(&[("a", Value::Float(x))])),
            Err(WriteError::NonFiniteFloat(_))
        ));
    }
}

#[test]
fn writer_nesting_deeper_than_128_is_an_error() {
    // Landed WP-002: the writer never produces text the reader rejects.
    let nest = |n: usize| {
        let mut v = Value::Null;
        for _ in 0..n {
            v = Value::Array(vec![v]);
        }
        v
    };
    assert!(write_compact(&nest(MAX_DEPTH)).is_ok());
    assert_eq!(write_compact(&nest(MAX_DEPTH + 1)), Err(WriteError::TooDeep));
    assert_eq!(write_pretty(&nest(MAX_DEPTH + 1)), Err(WriteError::TooDeep));
}

// --- §4 "Reading the writer's output must give back an equal value." -------

#[test]
fn round_trip_table() {
    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::Int(0),
        Value::Int(INT_MIN),
        Value::Int(INT_END - 1),
        Value::Int(i128::from(i64::MAX) + 1),
        Value::Float(1.0),
        Value::Float(-0.0),
        Value::Float(1e300),
        Value::Float(1e-300),
        Value::Float(0.1 + 0.2),
        Value::Float(18_446_744_073_709_551_616.0),
        s(""),
        s("\0\u{1}\u{1f}\"\\/\n\r\t\u{8}\u{c}\u{7f}é😀\u{10ffff}\u{feff}"),
        Value::Array(vec![]),
        Value::Object(vec![]),
        obj(&[("", Value::Null), ("b", Value::Int(1)), ("a", Value::Int(2))]),
        sample(),
    ];
    for v in &values {
        let compact = write_compact(v).unwrap();
        assert_eq!(&parse(compact.as_bytes()).unwrap(), v, "{compact}");
        let pretty = write_pretty(v).unwrap();
        assert_eq!(&parse(pretty.as_bytes()).unwrap(), v, "{pretty}");
    }
}

#[test]
fn round_trip_deterministic() {
    // §4 "both deterministic": the same value gives the same text, and
    // re-writing what was read gives the same text again.
    let v = sample();
    let text = write_pretty(&v).unwrap();
    assert_eq!(write_pretty(&v).unwrap(), text);
    assert_eq!(write_pretty(&parse(text.as_bytes()).unwrap()).unwrap(), text);
}
