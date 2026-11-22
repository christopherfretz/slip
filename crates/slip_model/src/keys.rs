//! Keys, names, rules and rule tables (`spec/digest.md` §3, the tables of
//! `spec/digest-kinds.md` §3.1 and §3.2). Work package WP-022.
//!
//! Plain data, their §2 encodings and the arithmetic on them: a key's id,
//! a rule table's id and the last step of mapping a key under a table. The
//! encodings are fixed by `spec/landed/model/WP-022.md`:
//!
//! - `KeyKind`: `uv`, its number. `Parent`: `uv` 0, 1, 2, or 3 followed by
//!   the `digest`. `Name`: `uv`, its number, then the "Then" column.
//! - `Key`: `kind`, `parent`, `name`, `disc` (`opt<digest>`).
//! - `Rule`: `parent`, `kind`, `name`, `disc`, `to_parent`, `to_name`;
//!   rules are ordered by the bytes of the encoding of their first four
//!   fields, their **match**.
//!
//! The derived `Ord` of [`Name`], [`Key`] and [`Rule`] is an order for maps
//! and sets, not the order of their encodings; [`RuleTable`] orders rules
//! by bytes as the spec says.

use std::cmp::Ordering;
use std::fmt;

use crate::Digest;
use crate::hash::tagged_digest;
use crate::wire::{Decoder, Encoder, Error, ErrorKind};

/// The tag of a `KeyId`: `H("slip.key.v1", Key)` (`spec/digest.md` §3).
pub const KEY_TAG: &str = "slip.key.v1";
/// The tag of a discriminator: `H("slip.disc.v1", stream)` (`spec/digest.md` §3.2).
pub const DISC_TAG: &str = "slip.disc.v1";
/// The tag of a rule table's id: `H("slip.rules.v1", list<Rule>)` (`spec/verify.md` §2).
pub const RULES_TAG: &str = "slip.rules.v1";
/// The most `unnamed_by` a `Name` may nest: one more is rejected.
pub const UNNAMED_BY_MAX: usize = 8;

fn malformed(offset: usize, what: &'static str) -> Error {
    Error::new(ErrorKind::Malformed, offset, what)
}

// ---------------------------------------------------------------------------
// KeyKind (digest-kinds §3.1)

/// What kind of entity a key names (`spec/digest-kinds.md` §3.1).
/// `ExplicitInstantiation` (24) is in the table and not used: it has a
/// variant so that reading it is not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum KeyKind {
    Namespace = 1,
    Class = 2,
    Enumeration = 3,
    Enumerator = 4,
    Function = 5,
    Variable = 6,
    Field = 7,
    Alias = 8,
    ClassTemplate = 9,
    FunctionTemplate = 10,
    VariableTemplate = 11,
    AliasTemplate = 12,
    Concept = 13,
    Specialisation = 14,
    PartialSpecialisation = 15,
    DeductionGuide = 16,
    UsingDeclaration = 17,
    UsingEnum = 18,
    NamespaceAlias = 19,
    Binding = 20,
    Decomposition = 21,
    UsingDirective = 22,
    StaticAssert = 23,
    ExplicitInstantiation = 24,
    Friend = 25,
    FileScopeAsm = 26,
    ConstevalBlock = 27,
    Resolved = 28,
    Order = 29,
    DefaultArgument = 30,
    TemplateParamObject = 31,
    UsingShadow = 32,
    UnresolvedIfExists = 33,
}

/// Every `KeyKind` with its number and its name in digest-kinds §3.1, in
/// number order.
pub(crate) const KEY_KINDS: [(KeyKind, u64, &str); 33] = [
    (KeyKind::Namespace, 1, "namespace"),
    (KeyKind::Class, 2, "class"),
    (KeyKind::Enumeration, 3, "enumeration"),
    (KeyKind::Enumerator, 4, "enumerator"),
    (KeyKind::Function, 5, "function"),
    (KeyKind::Variable, 6, "variable"),
    (KeyKind::Field, 7, "field"),
    (KeyKind::Alias, 8, "alias"),
    (KeyKind::ClassTemplate, 9, "class_template"),
    (KeyKind::FunctionTemplate, 10, "function_template"),
    (KeyKind::VariableTemplate, 11, "variable_template"),
    (KeyKind::AliasTemplate, 12, "alias_template"),
    (KeyKind::Concept, 13, "concept"),
    (KeyKind::Specialisation, 14, "specialisation"),
    (KeyKind::PartialSpecialisation, 15, "partial_specialisation"),
    (KeyKind::DeductionGuide, 16, "deduction_guide"),
    (KeyKind::UsingDeclaration, 17, "using_declaration"),
    (KeyKind::UsingEnum, 18, "using_enum"),
    (KeyKind::NamespaceAlias, 19, "namespace_alias"),
    (KeyKind::Binding, 20, "binding"),
    (KeyKind::Decomposition, 21, "decomposition"),
    (KeyKind::UsingDirective, 22, "using_directive"),
    (KeyKind::StaticAssert, 23, "static_assert"),
    (KeyKind::ExplicitInstantiation, 24, "explicit_instantiation"),
    (KeyKind::Friend, 25, "friend"),
    (KeyKind::FileScopeAsm, 26, "file_scope_asm"),
    (KeyKind::ConstevalBlock, 27, "consteval_block"),
    (KeyKind::Resolved, 28, "resolved"),
    (KeyKind::Order, 29, "order"),
    (KeyKind::DefaultArgument, 30, "default_argument"),
    (KeyKind::TemplateParamObject, 31, "template_param_object"),
    (KeyKind::UsingShadow, 32, "using_shadow"),
    (KeyKind::UnresolvedIfExists, 33, "unresolved_if_exists"),
];

impl KeyKind {
    /// Every value, in number order.
    pub const ALL: [KeyKind; 33] = {
        let mut all = [KeyKind::Namespace; 33];
        let mut i = 0;
        while i < 33 {
            all[i] = KEY_KINDS[i].0;
            i += 1;
        }
        all
    };

    /// Its number in digest-kinds §3.1.
    pub fn code(self) -> u64 {
        self as u64
    }

    /// The kind with number `code`, if the table lists it.
    pub fn from_code(code: u64) -> Option<KeyKind> {
        let i = usize::try_from(code.checked_sub(1)?).ok()?;
        KEY_KINDS.get(i).map(|t| t.0)
    }

    /// Its name in digest-kinds §3.1 (`class_template`), as in JSON.
    pub fn name(self) -> &'static str {
        KEY_KINDS[self as usize - 1].2
    }

    /// Appends its encoding: `uv`, its number.
    pub fn encode(self, e: &mut Encoder) {
        e.uv(self.code());
    }

    /// Reads one: `Malformed` at the value's first byte if unlisted.
    pub fn decode(d: &mut Decoder<'_>) -> Result<KeyKind, Error> {
        let at = d.offset();
        KeyKind::from_code(d.uv()?).ok_or_else(|| malformed(at, "unlisted KeyKind value"))
    }
}

// ---------------------------------------------------------------------------
// Parent (digest §3)

/// Who an entity is a member of (`spec/digest.md` §3). The derived order
/// is the order of the encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Parent {
    /// The global namespace.
    Root,
    /// Entities with C language linkage, whatever namespace declares them.
    CLinkage,
    /// Entities the front end declares itself.
    Builtin,
    /// The enclosing namespace, class, function or template: its `KeyId`.
    Unit(Digest),
}

impl Parent {
    /// Appends its encoding: `uv` 0, 1, 2, or 3 followed by the digest.
    pub fn encode(&self, e: &mut Encoder) {
        match self {
            Parent::Root => e.uv(0),
            Parent::CLinkage => e.uv(1),
            Parent::Builtin => e.uv(2),
            Parent::Unit(id) => e.uv(3).digest(id),
        };
    }

    /// Reads one: `Malformed` at the tag's first byte if it is above 3.
    pub fn decode(d: &mut Decoder<'_>) -> Result<Parent, Error> {
        let at = d.offset();
        match d.uv()? {
            0 => Ok(Parent::Root),
            1 => Ok(Parent::CLinkage),
            2 => Ok(Parent::Builtin),
            3 => Ok(Parent::Unit(d.digest()?)),
            _ => Err(malformed(at, "unlisted Parent value")),
        }
    }
}

// ---------------------------------------------------------------------------
// Name (digest §3.1, digest-kinds §3.2)

/// An entity's name (`spec/digest.md` §3.1, §3.3, §4.3; the numbers are
/// `spec/digest-kinds.md` §3.2).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Name {
    /// 1: the spelling.
    Identifier(Vec<u8>),
    /// 2: Slip's operator number (digest-kinds §10.2), not 0; not otherwise
    /// interpreted here.
    Operator(u64),
    /// 3.
    Constructor,
    /// 4.
    Destructor,
    /// 5: the digest of the type converted to.
    Conversion(Digest),
    /// 6: the suffix.
    LiteralOperator(Vec<u8>),
    /// 7.
    DeductionGuide,
    /// 8: the unnamed namespace.
    Anonymous,
    /// 9: keyed by content; `disc` is the unit's digest (digest §3.3).
    Content,
    /// 10: the ordinal among the unit's local types (digest §4.3).
    Local(u64),
    /// 11: the name that names an unnamed class or enumeration (digest
    /// §3.3). At most [`UNNAMED_BY_MAX`] of these nest.
    UnnamedBy(Box<Name>),
    /// 12: the ordinal among the unnamed types of the scope.
    UnnamedOrdinal(u64),
    /// 13: a resolved property (digest-kinds §11.1).
    Property(u64),
    /// 14.
    Order,
    /// 15: the parameter's index (a default argument unit).
    Parameter(u64),
}

/// The name of each `Name` form in digest-kinds §3.2, by number − 1.
pub(crate) const NAME_FORMS: [&str; 15] = [
    "identifier",
    "operator",
    "constructor",
    "destructor",
    "conversion",
    "literal_operator",
    "deduction_guide",
    "anonymous",
    "content",
    "local",
    "unnamed_by",
    "unnamed_ordinal",
    "property",
    "order",
    "parameter",
];

/// The `what` of a `Name` whose `unnamed_by` nest too deep.
pub(crate) const TOO_DEEP: &str = "unnamed_by nested more than 8 deep";
/// The `what` of an `operator` name with number 0.
pub(crate) const OPERATOR_ZERO: &str = "operator number is 0";

impl Name {
    /// Its number in digest-kinds §3.2.
    pub fn code(&self) -> u64 {
        match self {
            Name::Identifier(_) => 1,
            Name::Operator(_) => 2,
            Name::Constructor => 3,
            Name::Destructor => 4,
            Name::Conversion(_) => 5,
            Name::LiteralOperator(_) => 6,
            Name::DeductionGuide => 7,
            Name::Anonymous => 8,
            Name::Content => 9,
            Name::Local(_) => 10,
            Name::UnnamedBy(_) => 11,
            Name::UnnamedOrdinal(_) => 12,
            Name::Property(_) => 13,
            Name::Order => 14,
            Name::Parameter(_) => 15,
        }
    }

    /// The name of its form in digest-kinds §3.2 (`literal_operator`), as in JSON.
    pub fn form(&self) -> &'static str {
        NAME_FORMS[self.code() as usize - 1]
    }

    /// The rules a reader checks of a name in memory: an `operator` number
    /// other than 0, at most [`UNNAMED_BY_MAX`] nested `unnamed_by`.
    /// `Err` is the reader's message.
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut name = self;
        let mut depth = 0;
        loop {
            match name {
                Name::Operator(0) => return Err(OPERATOR_ZERO),
                Name::UnnamedBy(inner) => {
                    if depth == UNNAMED_BY_MAX {
                        return Err(TOO_DEEP);
                    }
                    depth += 1;
                    name = inner;
                }
                _ => return Ok(()),
            }
        }
    }

    /// Appends its encoding: `uv`, its number, then its value.
    pub fn encode(&self, e: &mut Encoder) {
        let mut name = self;
        loop {
            e.uv(name.code());
            match name {
                Name::Identifier(b) | Name::LiteralOperator(b) => {
                    e.bytes(b);
                }
                Name::Conversion(t) => {
                    e.digest(t);
                }
                Name::Operator(n)
                | Name::Local(n)
                | Name::UnnamedOrdinal(n)
                | Name::Property(n)
                | Name::Parameter(n) => {
                    e.uv(*n);
                }
                Name::UnnamedBy(inner) => {
                    name = inner;
                    continue;
                }
                Name::Constructor
                | Name::Destructor
                | Name::DeductionGuide
                | Name::Anonymous
                | Name::Content
                | Name::Order => {}
            }
            return;
        }
    }

    /// Reads one. `Malformed` at the number's first byte if it is unlisted
    /// or is the ninth nested `unnamed_by`; at the operator number's first
    /// byte if it is 0.
    pub fn decode(d: &mut Decoder<'_>) -> Result<Name, Error> {
        let mut wrappers = 0usize;
        let name = loop {
            let at = d.offset();
            let name = match d.uv()? {
                1 => Name::Identifier(d.bytes()?.to_vec()),
                2 => {
                    let at = d.offset();
                    match d.uv()? {
                        0 => return Err(malformed(at, OPERATOR_ZERO)),
                        n => Name::Operator(n),
                    }
                }
                3 => Name::Constructor,
                4 => Name::Destructor,
                5 => Name::Conversion(d.digest()?),
                6 => Name::LiteralOperator(d.bytes()?.to_vec()),
                7 => Name::DeductionGuide,
                8 => Name::Anonymous,
                9 => Name::Content,
                10 => Name::Local(d.uv()?),
                11 => {
                    if wrappers == UNNAMED_BY_MAX {
                        return Err(malformed(at, TOO_DEEP));
                    }
                    wrappers += 1;
                    continue;
                }
                12 => Name::UnnamedOrdinal(d.uv()?),
                13 => Name::Property(d.uv()?),
                14 => Name::Order,
                15 => Name::Parameter(d.uv()?),
                _ => return Err(malformed(at, "unlisted Name value")),
            };
            break name;
        };
        Ok((0..wrappers).fold(name, |inner, _| Name::UnnamedBy(Box::new(inner))))
    }
}

// ---------------------------------------------------------------------------
// Key (digest §3)

fn encode_disc(e: &mut Encoder, disc: Option<&Digest>) {
    match disc {
        None => e.u8(0),
        Some(d) => e.u8(1).digest(d),
    };
}

fn decode_disc(d: &mut Decoder<'_>) -> Result<Option<Digest>, Error> {
    let at = d.offset();
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(d.digest()?)),
        _ => Err(malformed(at, "opt tag is not 0 or 1")),
    }
}

/// The match of a rule or of a key: the encoding of `(parent, kind, name,
/// disc)`, in that order. Rules are ordered and looked up by these bytes.
fn encode_match(
    e: &mut Encoder,
    parent: &Parent,
    kind: KeyKind,
    name: &Name,
    disc: Option<&Digest>,
) {
    parent.encode(e);
    kind.encode(e);
    name.encode(e);
    encode_disc(e, disc);
}

fn match_bytes(parent: &Parent, kind: KeyKind, name: &Name, disc: Option<&Digest>) -> Vec<u8> {
    let mut e = Encoder::new();
    encode_match(&mut e, parent, kind, name, disc);
    e.into_bytes()
}

/// An entity's key (`spec/digest.md` §3).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub kind: KeyKind,
    pub parent: Parent,
    pub name: Name,
    /// The discriminator, `H("slip.disc.v1", stream)` (digest §3.2).
    pub disc: Option<Digest>,
}

impl Key {
    /// Appends its encoding: `kind`, `parent`, `name`, `disc`.
    pub fn encode(&self, e: &mut Encoder) {
        self.kind.encode(e);
        self.parent.encode(e);
        self.name.encode(e);
        encode_disc(e, self.disc.as_ref());
    }

    /// Reads one.
    pub fn decode(d: &mut Decoder<'_>) -> Result<Key, Error> {
        Ok(Key {
            kind: KeyKind::decode(d)?,
            parent: Parent::decode(d)?,
            name: Name::decode(d)?,
            disc: decode_disc(d)?,
        })
    }

    /// Its encoding.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        self.encode(&mut e);
        e.into_bytes()
    }

    /// Reads exactly one key: `Malformed` at the first leftover byte.
    pub fn from_bytes(bytes: &[u8]) -> Result<Key, Error> {
        let mut d = Decoder::new(bytes);
        let key = Key::decode(&mut d)?;
        d.finish()?;
        Ok(key)
    }

    /// Its `KeyId`, `H("slip.key.v1", encoding)`.
    pub fn id(&self) -> Digest {
        tagged_digest(KEY_TAG, &self.to_bytes())
    }
}

// ---------------------------------------------------------------------------
// Rule (digest §3.4, verify §2.1)

/// A relocation rule (`spec/digest.md` §3.4): the entity found as
/// `(parent, kind, name, disc)`, in mapped terms, is to be called
/// `(to_parent, to_name)` instead.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rule {
    pub parent: Parent,
    pub kind: KeyKind,
    pub name: Name,
    pub disc: Option<Digest>,
    pub to_parent: Parent,
    pub to_name: Name,
}

impl Rule {
    /// Appends its encoding: `parent`, `kind`, `name`, `disc`,
    /// `to_parent`, `to_name`.
    pub fn encode(&self, e: &mut Encoder) {
        encode_match(e, &self.parent, self.kind, &self.name, self.disc.as_ref());
        self.to_parent.encode(e);
        self.to_name.encode(e);
    }

    /// Reads one.
    pub fn decode(d: &mut Decoder<'_>) -> Result<Rule, Error> {
        Ok(Rule {
            parent: Parent::decode(d)?,
            kind: KeyKind::decode(d)?,
            name: Name::decode(d)?,
            disc: decode_disc(d)?,
            to_parent: Parent::decode(d)?,
            to_name: Name::decode(d)?,
        })
    }

    /// The encoding of its first four fields, by which rules are ordered.
    pub(crate) fn match_bytes(&self) -> Vec<u8> {
        match_bytes(&self.parent, self.kind, &self.name, self.disc.as_ref())
    }

    /// Its names as a reader checks them ([`Name::validate`]).
    fn validate(&self) -> Result<(), &'static str> {
        self.name.validate()?;
        self.to_name.validate()
    }
}

/// A list of rules that cannot be a rule table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuleError {
    /// Rules `first` and `second` (indices as given, `first < second`)
    /// have the same `(parent, kind, name, disc)`.
    Duplicate { first: usize, second: usize },
    /// Rule `index` has a name no reader accepts ([`Name::validate`]).
    InvalidName { index: usize, what: &'static str },
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::Duplicate { first, second } => {
                write!(f, "rules {first} and {second} have the same (parent, kind, name, disc)")
            }
            RuleError::InvalidName { index, what } => write!(f, "rule {index}: {what}"),
        }
    }
}

impl std::error::Error for RuleError {}

/// A rule table ready to apply (`spec/digest.md` §3.4): rules sorted by
/// the bytes of their match, no two with one match.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RuleTable {
    rules: Vec<Rule>,
    /// `rules[i].match_bytes()`, ascending.
    matches: Vec<Vec<u8>>,
}

impl RuleTable {
    /// No rule: mapping is the identity.
    pub fn empty() -> RuleTable {
        RuleTable::default()
    }

    /// The table of `rules`, given in any order. `Err`: a rule with a name
    /// no reader accepts, or two rules with the same `(parent, kind, name,
    /// disc)`.
    pub fn new(rules: Vec<Rule>) -> Result<RuleTable, RuleError> {
        for (index, rule) in rules.iter().enumerate() {
            rule.validate().map_err(|what| RuleError::InvalidName { index, what })?;
        }
        let mut order: Vec<(Vec<u8>, usize, Rule)> =
            rules.into_iter().enumerate().map(|(i, r)| (r.match_bytes(), i, r)).collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some(w) = order.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(RuleError::Duplicate { first: w[0].1, second: w[1].1 });
        }
        let (matches, rules) = order.into_iter().map(|(m, _, r)| (m, r)).unzip();
        Ok(RuleTable { rules, matches })
    }

    /// The table of `rules` already in order, with valid names and no two
    /// with one match: `Err((index, what))` names the first rule that is
    /// not, with the message a reader gives.
    pub(crate) fn from_sorted(rules: Vec<Rule>) -> Result<RuleTable, (usize, SortError)> {
        let mut matches: Vec<Vec<u8>> = Vec::with_capacity(rules.len());
        for (i, rule) in rules.iter().enumerate() {
            rule.validate().map_err(|what| (i, SortError::Invalid(what)))?;
            let m = rule.match_bytes();
            if let Some(prev) = matches.last() {
                match prev.cmp(&m) {
                    Ordering::Less => {}
                    Ordering::Equal => return Err((i, SortError::Duplicate)),
                    Ordering::Greater => return Err((i, SortError::Unsorted)),
                }
            }
            matches.push(m);
        }
        Ok(RuleTable { rules, matches })
    }

    /// The rules, ascending by the bytes of their `(parent, kind, name,
    /// disc)` (`spec/verify.md` §2).
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The last step of mapping (`spec/digest.md` §3.4): `key`, whose
    /// parent and discriminator are already mapped, with parent and name
    /// replaced by those of the rule that matches it, if one does; else
    /// `key` unchanged. Rules do not chain: this is one step.
    pub fn map(&self, key: Key) -> Key {
        if self.rules.is_empty() {
            return key;
        }
        let probe = match_bytes(&key.parent, key.kind, &key.name, key.disc.as_ref());
        match self.matches.binary_search_by(|m| m.as_slice().cmp(&probe)) {
            Ok(i) => {
                let rule = &self.rules[i];
                Key { parent: rule.to_parent, name: rule.to_name.clone(), ..key }
            }
            Err(_) => key,
        }
    }

    /// `H("slip.rules.v1", list<Rule>)` over the sorted rules: the table's
    /// `rules_id` (`spec/verify.md` §2).
    pub fn id(&self) -> Digest {
        let mut e = Encoder::new();
        e.count(self.rules.len());
        for rule in &self.rules {
            rule.encode(&mut e);
        }
        tagged_digest(RULES_TAG, e.as_bytes())
    }
}

/// Why a list of rules given in order is not a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SortError {
    /// A name no reader accepts.
    Invalid(&'static str),
    /// The same match as the rule before.
    Duplicate,
    /// A match below the rule before's.
    Unsorted,
}

/// The `what` of two rules with one match.
pub(crate) const DUPLICATE_RULE: &str = "two rules with one (parent, kind, name, disc)";
/// The `what` of rules out of order.
pub(crate) const UNSORTED_RULES: &str = "rules are not strictly ascending";

#[cfg(test)]
mod tests {
    use super::*;

    // digest-kinds §3.1: `ALL` is the table in number order.
    #[test]
    fn key_kind_table_is_in_number_order() {
        for (i, (kind, code, name)) in KEY_KINDS.iter().enumerate() {
            assert_eq!(kind.code(), *code);
            assert_eq!(*code, i as u64 + 1);
            assert_eq!(KeyKind::ALL[i], *kind);
            assert_eq!(kind.name(), *name);
            assert_eq!(KeyKind::from_code(*code), Some(*kind));
        }
        assert_eq!(KeyKind::from_code(0), None);
        assert_eq!(KeyKind::from_code(34), None);
        assert_eq!(KeyKind::from_code(u64::MAX), None);
    }
}
