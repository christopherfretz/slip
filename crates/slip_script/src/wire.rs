//! Layer L0 of the transform API as the host sees it (`spec/api.md` §3):
//! the operation table, the result word, handles and kind bits.
//!
//! Every number here is also in `resource/include/slip/__intrinsics.h`;
//! `tests/wire.rs` reads that header and checks that the two agree.

// ---------------------------------------------------------------------------
// Operation numbers (§3.2). A number is never given a different meaning.

pub const OP_ERROR_MESSAGE: u32 = 2;

pub const OP_LOOKUP: u32 = 16;
pub const OP_KIND: u32 = 17;
pub const OP_IDENTIFIER_OF: u32 = 18;
pub const OP_DISPLAY_STRING_OF: u32 = 19;
pub const OP_PARENT_OF: u32 = 20;
pub const OP_PARAMETERS_OF: u32 = 21;
pub const OP_DECLARATIONS_OF: u32 = 22;
pub const OP_ENTITY_OF: u32 = 23;
pub const OP_LOOKUP_IN: u32 = 24;

pub const OP_REFERENCES_TO: u32 = 32;
pub const OP_REFERENT_OF: u32 = 33;
pub const OP_NAME_RANGE_OF: u32 = 34;
pub const OP_EXTENT_OF: u32 = 35;
pub const OP_SOURCE_TEXT_OF: u32 = 36;
pub const OP_IS_EDITABLE: u32 = 37;

pub const OP_CALLS_TO: u32 = 48;
pub const OP_CALLEE_OF: u32 = 49;
pub const OP_CALLEE_REFERENCE_OF: u32 = 50;
pub const OP_ARGUMENTS_AS_WRITTEN: u32 = 51;

pub const OP_CODE_TEXT: u32 = 64;
pub const OP_CODE_SPELL: u32 = 65;
pub const OP_CODE_COPY: u32 = 66;
pub const OP_CODE_CAT: u32 = 67;
pub const OP_CODE_IDENTIFIER: u32 = 68;

pub const OP_REPLACE: u32 = 80;
pub const OP_NEW_GROUP: u32 = 81;
pub const OP_REPORT: u32 = 82;
pub const OP_EXPECT_CHANGED: u32 = 83;
pub const OP_EXPECT_RENAMED: u32 = 84;

// ---------------------------------------------------------------------------
// Statuses (§3.5, §4).

/// Why an operation was refused (`slip::errc`, §4). The value is the status
/// of the result word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Errc {
    InvalidHandle = 1,
    WrongKind = 2,
    InvalidArgument = 3,
    NotEditable = 4,
    Unsupported = 5,
    LimitExceeded = 6,
}

impl Errc {
    /// The status value (§3.3).
    pub fn status(self) -> u8 {
        self as u8
    }

    /// The name of §4.
    pub fn name(self) -> &'static str {
        match self {
            Errc::InvalidHandle => "invalid_handle",
            Errc::WrongKind => "wrong_kind",
            Errc::InvalidArgument => "invalid_argument",
            Errc::NotEditable => "not_editable",
            Errc::Unsupported => "unsupported",
            Errc::LimitExceeded => "limit_exceeded",
        }
    }
}

/// Status of a call to an operation number the host does not have (§3.5 rule 1).
pub const STATUS_UNKNOWN_OP: u8 = 254;
/// Status of a call whose arguments do not fit the operation (§3.5 rule 1).
pub const STATUS_MALFORMED: u8 = 255;

// ---------------------------------------------------------------------------
// The result word (§3.3).

/// The low 56 bits: a value, or a failure's ordinal.
pub const PAYLOAD_MASK: u64 = (1 << 56) - 1;
/// Each count of convention B is below this.
pub const COUNT_LIMIT: u64 = 1 << 28;

/// The word of a failure: `status << 56 | ordinal`.
pub fn failure_word(status: u8, ordinal: u64) -> u64 {
    debug_assert!(status != 0, "a failure has a nonzero status");
    u64::from(status) << 56 | (ordinal & PAYLOAD_MASK)
}

/// The word of a success of convention V.
pub fn value_word(value: u64) -> u64 {
    debug_assert!(value <= PAYLOAD_MASK, "a value of convention V fits 56 bits");
    value & PAYLOAD_MASK
}

/// The word of a success of convention B: `n_bytes << 28 | n_ints`.
pub fn counts_word(n_bytes: u64, n_ints: u64) -> u64 {
    debug_assert!(n_bytes < COUNT_LIMIT && n_ints < COUNT_LIMIT, "counts fit 28 bits");
    n_bytes << 28 | n_ints
}

/// The status of a result word; 0 is success.
pub fn status(word: u64) -> u8 {
    (word >> 56) as u8
}

/// The value (convention V) or the ordinal (a failure) of a result word.
pub fn payload(word: u64) -> u64 {
    word & PAYLOAD_MASK
}

/// The integer count of a result word of convention B.
pub fn n_ints(word: u64) -> u64 {
    word & (COUNT_LIMIT - 1)
}

/// The byte count of a result word of convention B.
pub fn n_bytes(word: u64) -> u64 {
    (word >> 28) & (COUNT_LIMIT - 1)
}

// ---------------------------------------------------------------------------
// Handles and kind bits (§3.4).

pub const TAG_ENTITY: u8 = 1;
pub const TAG_DECLARATION: u8 = 2;
pub const TAG_REFERENCE: u8 = 3;
pub const TAG_EXPRESSION: u8 = 4;
pub const TAG_CODE: u8 = 5;
pub const TAG_GROUP: u8 = 6;

/// Indices are 48 bits.
pub const INDEX_MASK: u64 = (1 << 48) - 1;

/// `tag << 48 | index`.
pub fn handle(tag: u8, index: u64) -> u64 {
    debug_assert!(index <= INDEX_MASK, "a handle index fits 48 bits");
    u64::from(tag) << 48 | (index & INDEX_MASK)
}

/// The tag of a handle, or `None` when bits above the tag are set.
pub fn handle_tag(h: u64) -> Option<u8> {
    if h >> 56 != 0 { None } else { Some((h >> 48) as u8) }
}

/// The index of a handle.
pub fn handle_index(h: u64) -> u64 {
    h & INDEX_MASK
}

pub const KIND_ENTITY: u64 = 1 << 0;
pub const KIND_DECLARATION: u64 = 1 << 1;
pub const KIND_REFERENCE: u64 = 1 << 2;
pub const KIND_EXPRESSION: u64 = 1 << 3;

pub const KIND_NAMESPACE: u64 = 1 << 8;
pub const KIND_TYPE: u64 = 1 << 9;
pub const KIND_FUNCTION: u64 = 1 << 10;
pub const KIND_VARIABLE: u64 = 1 << 11;
pub const KIND_TEMPLATE: u64 = 1 << 12;
pub const KIND_PARAMETER: u64 = 1 << 13;

pub const KIND_DEFINITION: u64 = 1 << 16;
pub const KIND_CALL: u64 = 1 << 17;

/// The kind bits only an entity may have (bits 8 to 13).
pub const KIND_ENTITY_BITS: u64 =
    KIND_NAMESPACE | KIND_TYPE | KIND_FUNCTION | KIND_VARIABLE | KIND_TEMPLATE | KIND_PARAMETER;

/// Every kind bit with a meaning (`(name, bit)`), in the order of §3.4,
/// named as `__intrinsics.h` names them without the `__kind_` prefix.
pub const KINDS: &[(&str, u64)] = &[
    ("entity", KIND_ENTITY),
    ("declaration", KIND_DECLARATION),
    ("reference", KIND_REFERENCE),
    ("expression", KIND_EXPRESSION),
    ("namespace", KIND_NAMESPACE),
    ("type", KIND_TYPE),
    ("function", KIND_FUNCTION),
    ("variable", KIND_VARIABLE),
    ("template", KIND_TEMPLATE),
    ("parameter", KIND_PARAMETER),
    ("definition", KIND_DEFINITION),
    ("call", KIND_CALL),
];

// ---------------------------------------------------------------------------
// The table (§3.2).

/// How many integers an operation takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arity {
    Exactly(usize),
    /// `code_cat`: any number.
    Any,
}

/// The result convention (§3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convention {
    V,
    B,
}

/// One row of §3.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    pub op: u32,
    /// The name of §3.2 (also `__op_<name>` in the header).
    pub name: &'static str,
    pub ints: Arity,
    /// Whether `bin` may hold text. An operation that takes none refuses
    /// any byte with status 255.
    pub text: bool,
    pub convention: Convention,
    /// Operations 64 to 84 (§7): they record, never answer.
    pub effect: bool,
}

impl Operation {
    /// Whether `ints_given` integers and `bytes_given` bytes fit the operation's shape.
    pub fn fits(&self, ints_given: usize, bytes_given: usize) -> bool {
        let ints = match self.ints {
            Arity::Exactly(n) => ints_given == n,
            Arity::Any => true,
        };
        ints && (self.text || bytes_given == 0)
    }
}

const fn row(
    op: u32,
    name: &'static str,
    ints: usize,
    text: bool,
    convention: Convention,
) -> Operation {
    Operation { op, name, ints: Arity::Exactly(ints), text, convention, effect: op >= 64 }
}

use Convention::{B, V};

/// Every operation, ascending by number.
pub const OPERATIONS: &[Operation] = &[
    row(OP_ERROR_MESSAGE, "error_message", 1, false, B),
    row(OP_LOOKUP, "lookup", 0, true, B),
    row(OP_KIND, "kind", 1, false, V),
    row(OP_IDENTIFIER_OF, "identifier_of", 1, false, B),
    row(OP_DISPLAY_STRING_OF, "display_string_of", 1, false, B),
    row(OP_PARENT_OF, "parent_of", 1, false, V),
    row(OP_PARAMETERS_OF, "parameters_of", 1, false, B),
    row(OP_DECLARATIONS_OF, "declarations_of", 1, false, B),
    row(OP_ENTITY_OF, "entity_of", 1, false, V),
    row(OP_LOOKUP_IN, "lookup_in", 1, true, B),
    row(OP_REFERENCES_TO, "references_to", 1, false, B),
    row(OP_REFERENT_OF, "referent_of", 1, false, V),
    row(OP_NAME_RANGE_OF, "name_range_of", 1, false, B),
    row(OP_EXTENT_OF, "extent_of", 1, false, B),
    row(OP_SOURCE_TEXT_OF, "source_text_of", 2, false, B),
    row(OP_IS_EDITABLE, "is_editable", 2, false, V),
    row(OP_CALLS_TO, "calls_to", 1, false, B),
    row(OP_CALLEE_OF, "callee_of", 1, false, V),
    row(OP_CALLEE_REFERENCE_OF, "callee_reference_of", 1, false, V),
    row(OP_ARGUMENTS_AS_WRITTEN, "arguments_as_written", 1, false, B),
    row(OP_CODE_TEXT, "code_text", 0, true, V),
    row(OP_CODE_SPELL, "code_spell", 1, false, V),
    row(OP_CODE_COPY, "code_copy", 2, false, V),
    Operation {
        op: OP_CODE_CAT,
        name: "code_cat",
        ints: Arity::Any,
        text: false,
        convention: V,
        effect: true,
    },
    row(OP_CODE_IDENTIFIER, "code_identifier", 0, true, V),
    row(OP_REPLACE, "replace", 4, true, V),
    row(OP_NEW_GROUP, "new_group", 0, false, V),
    row(OP_REPORT, "report", 4, true, V),
    row(OP_EXPECT_CHANGED, "expect_changed", 1, true, V),
    row(OP_EXPECT_RENAMED, "expect_renamed", 1, true, V),
];

/// The row of `op`, or `None` for a number the host does not have.
pub fn operation(op: u32) -> Option<&'static Operation> {
    OPERATIONS.binary_search_by_key(&op, |o| o.op).ok().map(|i| &OPERATIONS[i])
}
