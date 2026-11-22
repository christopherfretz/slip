//! Files, commands, observers and the plan (`spec/model.md` §6, §7).
//!
//! Plain data. The binary and JSON forms are implemented in `formats`
//! (work package WP-006).

use crate::Digest;

/// A file identified by its real path, with its size and content digest
/// (`spec/model.md` §6).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileRef {
    /// Real path: absolute, every symbolic link resolved. Not necessarily UTF-8.
    pub path: Vec<u8>,
    /// Size in bytes.
    pub size: u64,
    /// Content digest of the file's bytes.
    pub content: Digest,
}

/// One environment variable of a command.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvVar {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

/// A normalised compilation command (`spec/model.md` §7, `spec/cdb.md` §5).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    /// Working directory, absolute, as given by the database.
    pub directory: Vec<u8>,
    /// The normalised command line; `argv[0]` is the compiler as written.
    pub argv: Vec<Vec<u8>>,
    /// Environment variables the front end reads, sorted by name.
    pub env: Vec<EnvVar>,
}

/// Source language of a translation unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    C = 1,
    Cxx = 2,
    ObjC = 3,
    ObjCxx = 4,
}

/// Why a database entry did not become an observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reason {
    NotACompile = 1,
    UnsupportedLanguage = 2,
    UnsupportedFlag = 3,
    MissingFile = 4,
    MalformedEntry = 5,
    UnsupportedCompiler = 6,
}

/// A compilation database and the label it was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// Unique within the plan.
    pub name: String,
    /// The database file.
    pub database: FileRef,
}

/// Where an observer came from: entry `entry` of configuration `configuration`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Origin {
    /// Index into [`Plan::configurations`].
    pub configuration: u64,
    /// Zero-based index of the entry in that database.
    pub entry: u64,
}

/// One compilation to analyse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observer {
    /// `ObserverId` of `command`.
    pub id: Digest,
    pub command: Command,
    pub language: Language,
    /// The main source file.
    pub main: FileRef,
    /// Relative cost estimate; the main file's size when nothing better is known.
    pub cost_hint: u64,
    /// Sorted; at least one.
    pub origins: Vec<Origin>,
}

/// A database entry that did not become an observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excluded {
    pub configuration: u64,
    pub entry: u64,
    /// The entry's file, as spelled.
    pub file: Vec<u8>,
    pub reason: Reason,
    /// Human-readable; for `UnsupportedFlag`, the flag.
    pub detail: String,
}

/// The plan (`spec/model.md` §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Version of Slip that made the plan.
    pub tool_slip: String,
    /// Version of the Kiln front end it links.
    pub tool_kiln: String,
    pub configurations: Vec<Configuration>,
    /// In plan order: `cost_hint` descending, then `id` ascending.
    pub observers: Vec<Observer>,
    /// Sorted by (`configuration`, `entry`).
    pub excluded: Vec<Excluded>,
}
