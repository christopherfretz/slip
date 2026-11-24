//! The subject unit as the host sees it (`spec/api.md` §1, §5, §6, §7).
//!
//! The host is a pure function over a [`Subject`]: it validates what the
//! transform sends, asks the subject, checks what the subject answers and
//! encodes it. A subject names its values by small numbers of its own; the
//! host never shows them to the transform (it issues handles of its own,
//! §3.4), so a subject may choose them as it likes, under one contract:
//!
//! **One number per value, per sort, for the life of the `Host`.** The
//! host gives a value the same handle every time it meets the same number,
//! and that is the whole of `info` identity (§5.1 rule 2: two `info` are
//! equal exactly when they denote the same thing). So a subject must give
//! each entity (declaration, reference, expression, file) exactly one
//! number, return that number from every method that yields the value,
//! and never renumber while a `Host` holds it. Two numbers for one entity
//! make two unequal handles for one entity: a defect of the subject that
//! the host cannot see. Numbers of different sorts are independent
//! (`Entity(3)` and `Declaration(3)` are unrelated).

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

/// Where a report about a value goes (§7.4 rule 3): `Subject::place`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceOf {
    /// The value's own place: a range of tier 0 or 1.
    Own(Range),
    /// The nearest place the host can name (the macro invocation that
    /// produced the value), the value's own being of no tier 0 or 1.
    Nearest(Range),
    /// The value has a place, and the host can name neither it nor a
    /// nearer one.
    Unnamed,
    /// Nothing to attach to: an entity with no written declaration.
    Nowhere,
}

/// Why an entity that has an identifier has no qualified name without
/// template arguments (`spec/api.md` §7.1 rule 4): `Subject::unqualified`.
/// `spell` of it is made all the same; the piece holds the entity, and
/// its text is the entity's identifier where the place takes one (§7.2
/// rule 1), refused where the place takes the fully qualified spelling
/// (§7.2 rule 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Unqualified {
    /// A specialisation or a partial specialisation of a template.
    Specialisation,
    /// A member of a class template specialisation, at any depth.
    MemberOfSpecialisation,
    /// A member of a class template (of its pattern), at any depth.
    MemberOfClassTemplate,
    /// An entity declared in an unnamed class.
    InUnnamedClass,
    /// An entity whose only declarations are friend declarations:
    /// qualified lookup does not find it ([namespace.memdef]/3).
    OnlyFriends,
}

impl Unqualified {
    /// What the entity is, for a message.
    pub fn what(self) -> &'static str {
        match self {
            Unqualified::Specialisation => "a specialisation",
            Unqualified::MemberOfSpecialisation => "a member of a specialisation",
            Unqualified::MemberOfClassTemplate => "a member of a class template",
            Unqualified::InUnnamedClass => "declared in an unnamed class",
            Unqualified::OnlyFriends => "declared only by friend declarations",
        }
    }
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
///
/// **Statuses a subject may choose.** Where a method returns `Err(errc)`
/// or `None` and the host passes it on as a status, the specification
/// allows only the statuses listed on the method below. The host turns
/// any other into a host failure naming the method and the status; a
/// method with no list may not refuse at all for the sorts the host asks
/// it about (the host refuses the other sorts itself, before asking).
///
/// Every number the subject returns follows the numbering contract of the
/// module documentation.
pub trait Subject {
    fn is_live(&self, v: Value) -> bool;
    fn file_len(&self, f: File) -> Option<u32>;
    fn kind_bits(&self, v: Value) -> u64;
    fn lookup(&self, global: bool, components: &[&[u8]]) -> Vec<Entity>;
    /// §6.1, `lookup(scope, identifier)`: the entities declared with
    /// `identifier` as members of `scope` (the members of the scopes it
    /// contains transparently included, §6.1 rule 3), found as `lookup`
    /// finds the last component of a qualified name that reached `scope`
    /// (rules 3 to 7), in unit order of their first declarations; for a
    /// typedef name for linkage purposes, the members of the class it
    /// names (rule 4). `None`: `scope` is not a namespace, a class, a
    /// class template, a specialisation of one or an enumeration, which
    /// the host refuses with `wrong_kind`. `identifier` is one plain
    /// identifier (the host checked it).
    fn lookup_in(&self, scope: Entity, identifier: &[u8]) -> Option<Vec<Entity>>;
    /// §6.3 rule 1. `None` only for an expression, which the host never
    /// asks about: `None` for any other sort is a host failure. An empty
    /// identifier is `Some(vec![])`.
    fn identifier(&self, v: Value) -> Option<Vec<u8>>; // None: an expression
    fn display(&self, v: Value) -> String;
    fn parent(&self, e: Entity) -> Option<Entity>;
    /// §6.4 rule 2, for an entity or a declaration. `None` becomes
    /// `wrong_kind` (not a function, a function template, or a
    /// declaration of one).
    fn parameters(&self, v: Value) -> Option<Vec<Entity>>; // None: wrong kind
    /// §6.4 rule 3. Allowed: `Err(Unsupported)` (a parameter).
    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc>;
    fn entity_of(&self, d: Declaration) -> Entity;
    fn origin(&self, e: Entity) -> Entity; // spec/api.md §5.5
    fn references_to(&self, e: Entity) -> Vec<Reference>;
    fn referent(&self, r: Reference) -> Entity;
    /// §6.6 rule 1, asked only for a declaration or a reference. No `Err`
    /// is allowed.
    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc>;
    /// §6.6 rules 2 to 4, asked only for a reference or an expression. No
    /// `Err` is allowed.
    fn extent(&self, v: Value) -> Result<Option<Range>, Errc>;
    fn text(&self, r: Range) -> &[u8];
    fn owned(&self, f: File) -> bool;
    /// §6.7 rule 3. Allowed: `Err(WrongKind)` (not a function or a
    /// function template).
    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc>;
    /// §6.7 rule 4. `None` becomes `wrong_kind` (not a call, rule 6); so
    /// for the next two.
    fn callee(&self, x: Expression) -> Option<Entity>; // None: not a call
    fn callee_reference(&self, x: Expression) -> Option<Reference>;
    fn arguments(&self, x: Expression) -> Option<Vec<Expression>>;
    /// §6.5 rule 6: how many unresolved names of the unit have this identifier; None: the subject cannot tell.
    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64>;
    /// How many key questions (§7.5) the subject answered outside the
    /// run's own key server, each at the cost of the unit's keys built
    /// again (`spec/landed/api/WP-095.md`): the counter
    /// `api.one_shot_key_servers`, written when not 0.
    fn one_shot_key_servers(&self) -> u64 {
        0
    }

    // What the effects need (§7). WP-088 is their only caller.
    /// §5.4, §7.6: the tiers at which the unit reaches `r` when `r` is
    /// exactly a range that `name_range` or `extent` gives for some value
    /// of the unit (the only queries that return a range, §6.6), as
    /// `reached[t]` for tier `t`; `None` for every other range, including
    /// one that lies inside such a range. Both can be true: one range can
    /// be reached at tier 0 by one value and at tier 1 by another (a
    /// header included twice, a name that is a macro in one inclusion and
    /// a function in the other). `None` makes `replace` refuse with
    /// `invalid_handle` (§7.3 rule 2, §5.2 rule 2); `Some([false, false])`,
    /// the empty set, is a host failure.
    fn tiers(&self, r: Range) -> Option<[bool; 2]>;
    /// §7.2 rule 2: the identifiers from the outermost named scope to `e`; None: §7.1 rule 4.
    /// `Some` must hold one identifier or more, none empty, the last
    /// `identifier(e)`; anything else is a host failure.
    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>>;
    /// §7.1 rule 4, asked only when `qualified(e)` is `None`: why `e`
    /// has no qualified name, when it is spelled all the same (its
    /// identifier where it lands, §7.2 rule 1); `None`: it is never
    /// spelled (a parameter of a function or a template, no identifier,
    /// an entity with no written declaration that is made from no written
    /// entity, a declaration in a function body, a using-declaration, a
    /// deduction guide, a structured binding), and `spell` refuses.
    /// `Some` for an entity whose identifier is empty is a host failure.
    fn unqualified(&self, e: Entity) -> Option<Unqualified>;
    /// §7.2 rule 1: the references whose extent is exactly `r`. Each must
    /// be live, listed once, and have `extent` `Some(r)`, and a name range
    /// that is not empty and lies in `r`; anything else is a host failure.
    fn references_at(&self, r: Range) -> Vec<Reference>;
    /// §7.2 rule 1 at a name range (`spec/landed/api/WP-132.md`): the
    /// references whose name range is exactly `r`. Each must be live,
    /// listed once and have `name_range` `Some(r)`; anything else is a
    /// host failure.
    fn references_named_at(&self, r: Range) -> Vec<Reference>;
    /// §7.2 rule 1 at a declaration's name: the declarations whose name
    /// range is exactly `r`, under the same contract.
    fn declarations_named_at(&self, r: Range) -> Vec<Declaration>;
    /// §7.2 rule 1, third condition.
    fn found_directly(&self, r: Reference) -> bool;
    /// The key of `e` as an entity (for a spelled piece); None: it has none, and `spell` refuses.
    /// (`None` becomes `unsupported`, §7.1 rule 4; so does `None` from `qualified`.)
    fn entity_key(&self, e: Entity) -> Option<slip_model::Digest>;
    /// §7.5 rule 1: the key of the unit that holds `v` (a reference or an expression;
    /// the host refuses the other sorts before asking), then the keys derived from it, sorted.
    /// Allowed: `Err(Unsupported)` (§7.5 rule 2). `Ok` with no key, or
    /// with a key twice, is a host failure.
    fn unit_keys(&self, v: Value) -> Result<Vec<slip_model::Digest>, Errc>;
    /// §7.5 rule 3. Allowed: `Err(Unsupported)` (not a unit keyed by name).
    /// The rule is the one that undoes the rename (`spec/verify.md` §2.1):
    /// its `name` must be `new_identifier`, or it is a host failure.
    fn rename_rule(&self, e: Entity, new_identifier: &[u8])
    -> Result<slip_model::keys::Rule, Errc>;
    /// §7.4 rule 3: the place of `v` for a report. A range outside its
    /// file is a host failure when a report asks.
    fn place(&self, v: Value) -> PlaceOf;
    fn file_ref(&self, f: File) -> slip_model::FileRef;
}
