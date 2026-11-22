//! Reading a JSON compilation database (`spec/cdb.md` §2). Work package WP-007.
//!
//! [`read_json`] turns the database's bytes into one result per entry, in
//! order: an [`Entry`] when the element is well formed, a [`Defect`] saying
//! why not otherwise. One bad entry never fails the database (§1 principle
//! 2); only bytes that are not JSON, or JSON whose top level is not an array,
//! are a [`DatabaseError`].
//!
//! Readers for other formats produce the same `Vec<Result<Entry, Defect>>`
//! and share everything from §4 on ([`crate::plan`]).

use std::fmt;
use std::path::Path;

use slip_model::json::{self, Value};

use crate::split::split_command;

/// A well-formed entry (§2): its working directory, main file and command
/// line, already split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Absolute, as given.
    pub directory: String,
    /// The main file as spelled: absolute or relative to `directory`.
    pub file: String,
    /// The command line: `arguments` as given, or `command` split by §3.
    /// Never empty.
    pub arguments: Vec<String>,
}

/// Why an entry is not well formed (§2). The entry is excluded as
/// `malformed_entry` with `detail`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defect {
    /// The entry's `file` as spelled; empty if absent or not a string.
    pub file: String,
    /// Human-readable.
    pub detail: String,
}

/// The database as a whole cannot be read (§2): it is not JSON, or its top
/// level is not an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseError {
    pub detail: String,
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for DatabaseError {}

/// Reads a JSON compilation database (§2). One element per entry, in order;
/// an entry's number is its index.
pub fn read_json(bytes: &[u8]) -> Result<Vec<Result<Entry, Defect>>, DatabaseError> {
    let value =
        json::parse(bytes).map_err(|e| DatabaseError { detail: format!("not JSON: {e}") })?;
    let Value::Array(elements) = value else {
        return Err(DatabaseError {
            detail: format!("the top level is {}, not an array", kind(&value)),
        });
    };
    Ok(elements.iter().map(entry).collect())
}

/// One element of the top-level array.
fn entry(value: &Value) -> Result<Entry, Defect> {
    let Value::Object(_) = value else {
        return Err(Defect {
            file: String::new(),
            detail: format!("the entry is {}, not an object", kind(value)),
        });
    };
    // `file` first, so every later defect can carry it.
    let file = value.get("file").and_then(Value::as_str).unwrap_or("").to_owned();
    let defect = |detail: String| Defect { file: file.clone(), detail };

    let directory = required_string(value, "directory").map_err(defect)?;
    if !Path::new(directory).is_absolute() {
        return Err(defect(format!("`directory` `{directory}` is not an absolute path")));
    }
    required_string(value, "file").map_err(defect)?;

    let arguments = match (value.get("arguments"), value.get("command")) {
        (Some(arguments), _) => arguments_array(arguments).map_err(defect)?,
        (None, Some(Value::Str(command))) => split_command(command).map_err(|e| {
            defect(format!("`command` cannot be split: {} (byte {})", e.message, e.offset))
        })?,
        (None, Some(other)) => {
            return Err(defect(format!("`command` is {}, not a string", kind(other))));
        }
        (None, None) => return Err(defect("neither `arguments` nor `command` is present".into())),
    };
    Ok(Entry { directory: directory.to_owned(), file, arguments })
}

/// The string member `key`, which must be present.
fn required_string<'v>(object: &'v Value, key: &str) -> Result<&'v str, String> {
    match object.get(key) {
        None => Err(format!("`{key}` is missing")),
        Some(Value::Str(s)) => Ok(s),
        Some(other) => Err(format!("`{key}` is {}, not a string", kind(other))),
    }
}

/// `arguments`: an array of strings, not empty.
fn arguments_array(value: &Value) -> Result<Vec<String>, String> {
    let Value::Array(elements) = value else {
        return Err(format!("`arguments` is {}, not an array", kind(value)));
    };
    if elements.is_empty() {
        return Err("`arguments` is empty".into());
    }
    elements
        .iter()
        .enumerate()
        .map(|(i, element)| match element {
            Value::Str(s) => Ok(s.clone()),
            other => Err(format!("`arguments[{i}]` is {}, not a string", kind(other))),
        })
        .collect()
}

/// The JSON type of a value, with its article, for messages.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Int(_) | Value::Float(_) => "a number",
        Value::Str(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
