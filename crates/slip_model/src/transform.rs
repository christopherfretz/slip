//! The transform request, kind `TRFM` (`spec/edits.md` §3), and the owned
//! set it carries (`spec/edits.md` §2.1). Work package WP-060.
//!
//! Plain data. The binary and JSON forms, and the rules a reader checks,
//! are in `formats` (`spec/landed/edits/WP-060.md`).

use crate::Digest;

/// The files Slip may edit (`spec/edits.md` §2.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ownership {
    /// Canonical absolute real paths ([`is_canonical`]) of directories or
    /// files; sorted, no two equal, not empty.
    pub owned: Vec<Vec<u8>>,
    /// The same; sorted, no two equal, none also in `owned`.
    pub readonly: Vec<Vec<u8>>,
}

/// `entry` is a canonical absolute path (`spec/edits.md` §2.1): it starts
/// with `/`, has no empty, `.` or `..` component, no NUL byte, and has no
/// trailing `/` unless it is `/` itself.
pub fn is_canonical(entry: &[u8]) -> bool {
    if entry.contains(&0) {
        return false;
    }
    match entry.strip_prefix(b"/") {
        None => false,
        Some(b"") => true,
        Some(rest) => rest.split(|&b| b == b'/').all(|c| !matches!(c, b"" | b"." | b"..")),
    }
}

/// `path` is a canonical absolute path that can name a file
/// (`spec/edits.md` §2.1): [`is_canonical`], and not `/`.
pub fn is_file_path(path: &[u8]) -> bool {
    path != b"/" && is_canonical(path)
}

/// `entry` covers `path` (`spec/edits.md` §2.1): the entry `/` covers every
/// absolute path; any other entry covers `path` iff `path` equals it or
/// starts with it followed by `/`. `/src` covers `/src/a.cc`; `/src/a` does
/// not cover `/src/ab.cc`.
pub fn covers(entry: &[u8], path: &[u8]) -> bool {
    if entry == b"/" {
        return path.first() == Some(&b'/');
    }
    match path.strip_prefix(entry) {
        None => false,
        Some(rest) => rest.is_empty() || rest[0] == b'/',
    }
}

impl Ownership {
    /// `spec/edits.md` §2.1: some entry covers `real_path` and the longest
    /// entry covering it is in `owned`. A path no entry covers, and one
    /// whose longest covering entry is in `readonly`, is read-only.
    pub fn is_owned(&self, real_path: &[u8]) -> bool {
        let longest =
            |list: &[Vec<u8>]| list.iter().filter(|e| covers(e, real_path)).map(Vec::len).max();
        match (longest(&self.owned), longest(&self.readonly)) {
            (None, _) => false,
            (Some(_), None) => true,
            // `owned` and `readonly` share no entry, and two entries that
            // cover one path and have one length are equal.
            (Some(o), Some(r)) => o > r,
        }
    }
}

/// What the request asks for (`spec/edits.md` §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransformKind {
    Rename = 1,
    Script = 2,
}

/// Which entities a rename's target may name (`spec/edits.md` §3, §9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    Any = 0,
    Function = 1,
    Variable = 2,
}

/// What a rename does with a dependent name that an instantiation the
/// observer made binds to a subject (`spec/edits.md` §3, §9.4; added by
/// `docs/wp/WP-139.md`, item 4; `leave` and the default by its second
/// review).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dependent {
    /// The name is edited when every instantiation of the observer binds
    /// it to a subject (§9.4 row 2). The run cannot verify such an edit
    /// until dependent names are compared mapped (its holder differs).
    Offer = 0,
    /// The name is refused (`refuse(instantiated)`), for a run whose
    /// observers are known not to be the program (a library's own tests).
    Refuse = 1,
    /// Row 2 is not read: every dependent name is `unresolved` (residue
    /// `dependent`, §8.4), as before WP-139. The default until dependent
    /// names are compared mapped.
    #[default]
    Leave = 2,
}

/// A native rename (`spec/edits.md` §3, §9).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rename {
    /// The qualified name, outermost component first; at least one.
    pub target: Vec<Vec<u8>>,
    pub kind: TargetKind,
    /// An identifier, other than the last component of `target`.
    pub new_name: Vec<u8>,
    /// Since version 2 (`docs/wp/WP-139.md`).
    pub dependent: Dependent,
}

/// A programmable transform (`spec/edits.md` §3, `spec/api.md` §2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Script {
    /// The `TransformId` of `spec/api.md` §2.2.
    pub id: Digest,
    /// The transform's source file, as given.
    pub file: Vec<u8>,
    /// The author's options (`spec/api.md` §2.1 rule 2), in the order given.
    pub options: Vec<Vec<u8>>,
    /// The budgets of `spec/api.md` §2.5.
    pub steps: u64,
    pub bytes: u64,
    pub depth: u64,
}

/// A transform request, kind `TRFM`, version 2 (`spec/edits.md` §3). Its
/// content id is the request id a map job and a map shard name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Transform {
    pub kind: TransformKind,
    pub ownership: Ownership,
    /// Every edit of the run is one atomic group (`spec/edits.md` §5.5);
    /// true for a rename.
    pub indivisible: bool,
    /// Present iff `kind` is `Rename`.
    pub rename: Option<Rename>,
    /// Present iff `kind` is `Script`.
    pub script: Option<Script>,
}

/// Every keyword of C++98 to C++26, and the alternative representations,
/// which are reserved as keywords are ([lex.key]/2): no component of a
/// rename's target and no new name may be one (`spec/edits.md` §3).
/// Identifiers with special meaning (`final`, `override`, `import`,
/// `module`, [lex.name] Table 4) are not keywords and are allowed.
/// C++98 has 63, C++11 adds 10, C++20 8, C++26 1; with the 11 alternative
/// representations, 93.
pub(crate) const KEYWORDS: [&str; 93] = [
    // [lex.key] Table 5, C++98 (ISO/IEC 14882:1998 2.11 Table 3).
    "asm",
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "const_cast",
    "continue",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "operator",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    // [lex.key] Table 5, added by C++11 (ISO/IEC 14882:2011 2.12 Table 4).
    "alignas",
    "alignof",
    "char16_t",
    "char32_t",
    "constexpr",
    "decltype",
    "noexcept",
    "nullptr",
    "static_assert",
    "thread_local",
    // [lex.key] Table 5, added by C++20 (ISO/IEC 14882:2020 5.11 Table 5).
    "char8_t",
    "co_await",
    "co_return",
    "co_yield",
    "concept",
    "consteval",
    "constinit",
    "requires",
    // [lex.key] Table 5, added by C++26 (P2900, contracts).
    "contract_assert",
    // [lex.key]/2 Table 6 ([lex.digraph]): the alternative representations,
    // reserved since C++98.
    "and",
    "and_eq",
    "bitand",
    "bitor",
    "compl",
    "not",
    "not_eq",
    "or",
    "or_eq",
    "xor",
    "xor_eq",
];

/// `name` is one of [`KEYWORDS`].
pub(crate) fn is_keyword(name: &[u8]) -> bool {
    KEYWORDS.iter().any(|k| k.as_bytes() == name)
}

/// `name` is one plain identifier (`spec/api.md` §6.1 rule 1): it matches
/// `[A-Za-z_][A-Za-z0-9_]*` and is not one of [`KEYWORDS`].
pub(crate) fn is_plain(name: &[u8]) -> bool {
    is_identifier(name) && !is_keyword(name)
}

/// `name` matches `[A-Za-z_][A-Za-z0-9_]*` (`spec/edits.md` §3).
pub(crate) fn is_identifier(name: &[u8]) -> bool {
    match name.split_first() {
        None => false,
        Some((first, rest)) => {
            (first.is_ascii_alphabetic() || *first == b'_')
                && rest.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        }
    }
}
