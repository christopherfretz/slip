//! The subject unit as the host sees it (`spec/api.md` §1, §5, §6, §7).
//!
//! The host is a pure function over a [`Subject`]: it validates what the
//! transform sends, asks the subject, checks what the subject answers and
//! encodes it. A subject names its values by small numbers of its own; the
//! host never shows them to the transform (it issues handles of its own,
//! §3.4), so a subject may number them as it likes.

use crate::wire::Errc;

/// An entity of the subject unit (§5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(pub u32);

/// One written declaration (§5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Declaration(pub u32);

/// One written reference (§5.1, §6.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reference(pub u32);

/// One written expression (§5.1, §6.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Expression(pub u32);

/// A file of the subject unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct File(pub u32);

/// A half-open run of bytes of one file (§5.4): `lo <= hi <=` its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Range {
    pub file: File,
    pub lo: u32,
    pub hi: u32,
}

/// An `info` (§5.1): a value of one of the four sorts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    Entity(Entity),
    Declaration(Declaration),
    Reference(Reference),
    Expression(Expression),
}

impl Value {
    /// The sort's tag of §3.4 (1 to 4).
    pub fn tag(self) -> u8 {
        match self {
            Value::Entity(_) => crate::wire::TAG_ENTITY,
            Value::Declaration(_) => crate::wire::TAG_DECLARATION,
            Value::Reference(_) => crate::wire::TAG_REFERENCE,
            Value::Expression(_) => crate::wire::TAG_EXPRESSION,
        }
    }

    /// The subject's number for the value.
    pub fn number(self) -> u32 {
        match self {
            Value::Entity(Entity(n))
            | Value::Declaration(Declaration(n))
            | Value::Reference(Reference(n))
            | Value::Expression(Expression(n)) => n,
        }
    }

    /// The value of sort `tag` (1 to 4) numbered `n`.
    pub fn from_tag(tag: u8, n: u32) -> Option<Value> {
        Some(match tag {
            crate::wire::TAG_ENTITY => Value::Entity(Entity(n)),
            crate::wire::TAG_DECLARATION => Value::Declaration(Declaration(n)),
            crate::wire::TAG_REFERENCE => Value::Reference(Reference(n)),
            crate::wire::TAG_EXPRESSION => Value::Expression(Expression(n)),
            _ => return None,
        })
    }

    /// The sort bit of §3.4.
    pub fn sort_bit(self) -> u64 {
        1 << (self.tag() - 1)
    }

    /// The sort's name, for messages.
    pub fn sort_name(self) -> &'static str {
        match self {
            Value::Entity(_) => "an entity",
            Value::Declaration(_) => "a declaration",
            Value::Reference(_) => "a reference",
            Value::Expression(_) => "an expression",
        }
    }
}

/// One subject unit, as the host sees it. Every sequence is returned in
/// the order spec/api.md gives; the host does not sort.
///
/// The host asks a method only with values the subject said were live and
/// of the sort the method names; it checks every value, range and
/// sequence the subject returns (live, in bounds, no duplicate) and treats
/// a violation as a host failure (§2.8), never as the transform's fault.
pub trait Subject {
    fn is_live(&self, v: Value) -> bool;
    fn file_len(&self, f: File) -> Option<u32>;
    fn kind_bits(&self, v: Value) -> u64;
    fn lookup(&self, global: bool, components: &[&[u8]]) -> Vec<Entity>;
    fn identifier(&self, v: Value) -> Option<Vec<u8>>; // None: an expression
    fn display(&self, v: Value) -> String;
    fn parent(&self, e: Entity) -> Option<Entity>;
    fn parameters(&self, v: Value) -> Option<Vec<Entity>>; // None: wrong kind
    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc>;
    fn entity_of(&self, d: Declaration) -> Entity;
    fn origin(&self, e: Entity) -> Entity; // spec/api.md §5.5
    fn references_to(&self, e: Entity) -> Vec<Reference>;
    fn referent(&self, r: Reference) -> Entity;
    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc>;
    fn extent(&self, v: Value) -> Result<Option<Range>, Errc>;
    fn text(&self, r: Range) -> &[u8];
    fn owned(&self, f: File) -> bool;
    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc>;
    fn callee(&self, x: Expression) -> Option<Entity>; // None: not a call
    fn callee_reference(&self, x: Expression) -> Option<Reference>;
    fn arguments(&self, x: Expression) -> Option<Vec<Expression>>;

    // What the effects need (§7). WP-088 is their only caller.
    /// §5.4: 0 or 1 if `r` is a range the subject gave out or lies inside one; else None.
    fn tier(&self, r: Range) -> Option<u8>;
    /// §7.2 rule 2: the identifiers from the outermost named scope to `e`; None: §7.1 rule 4.
    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>>;
    /// §7.2 rule 1: the references whose extent is exactly `r`.
    fn references_at(&self, r: Range) -> Vec<Reference>;
    /// §7.2 rule 1, third condition.
    fn found_directly(&self, r: Reference) -> bool;
    /// The key of `e` as an entity (for a spelled piece); None: it has none, and `spell` refuses.
    fn entity_key(&self, e: Entity) -> Option<slip_model::Digest>;
    /// §7.5 rule 1: the key of the unit that holds `v` (a reference or an expression;
    /// the host refuses the other sorts before asking), then the keys derived from it, sorted.
    fn unit_keys(&self, v: Value) -> Result<Vec<slip_model::Digest>, Errc>;
    /// §7.5 rule 3.
    fn rename_rule(&self, e: Entity, new_identifier: &[u8])
    -> Result<slip_model::keys::Rule, Errc>;
    /// §7.4 rule 3: the place, and whether it is the value's own (`false`: the nearest nameable place).
    fn place(&self, v: Value) -> Option<(Range, bool)>;
    fn file_ref(&self, f: File) -> slip_model::FileRef;
}
