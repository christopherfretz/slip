//! A subject held in memory, for tests of the host (`spec/api.md` §3 to §7)
//! that need no Kiln.
//!
//! A [`FakeSubject`] is described literally: files with their text, then
//! entities, declarations, references and expressions, each listed **in
//! unit order** (§1). The fake answers every method of [`Subject`] from
//! that description by the rules of the specification where they are
//! cheap to state (lookup by declaration, §6.1; origins, §5.5; the
//! spelling conditions, §7.1 rule 4 and §7.2 rule 2) and from fields of
//! the description where they are not (keys, rules, tiers).

use slip_model::keys::{KeyKind, Name, Parent, Rule};
use slip_model::{Digest, FileRef};

use crate::host::Out;
use crate::subject::{Declaration, Entity, Expression, File, Range, Reference, Subject, Value};
use crate::wire::{self, Errc};

/// A file of the fake subject.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FakeFile {
    /// The real path its `FileRef` carries.
    pub path: Vec<u8>,
    pub text: Vec<u8>,
    /// Whether the run may edit it (§6.6 rule 6).
    pub owned: bool,
}

/// An entity of the fake subject.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FakeEntity {
    /// Empty: the entity has no identifier.
    pub identifier: Vec<u8>,
    /// `display_string_of`; empty: derived from the identifier.
    pub display: String,
    /// Kind bits 8 to 13 (`wire::KIND_NAMESPACE` ...); the sort bit is added.
    pub kinds: u64,
    /// `None` only for the global namespace.
    pub parent: Option<Entity>,
    /// §5.5; `None`: the entity is its own origin (it was written).
    pub origin: Option<Entity>,
    /// A scope whose members `lookup` walks into (§6.1 rule 4).
    pub scope: bool,
    /// A scope whose members are also members of its parent for `lookup`
    /// (§6.1 rule 3: inline and unnamed namespaces, unscoped enumerations,
    /// anonymous unions).
    pub transparent: bool,
    /// `parameters_of`: `Some` for a function or a function template.
    pub parameters: Option<Vec<Entity>>,
    /// `Subject::entity_key`.
    pub key: Option<Digest>,
    /// `Subject::rename_rule`: the entity's `(parent, kind, disc)` as a
    /// key, from which the fake builds the rule that undoes a rename;
    /// `None`: not a unit keyed by name, `unsupported`.
    pub rule: Option<(Parent, KeyKind, Option<Digest>)>,
}

impl FakeEntity {
    /// A named namespace in `parent`.
    pub fn namespace(identifier: &str, parent: Entity) -> FakeEntity {
        FakeEntity {
            identifier: identifier.as_bytes().to_vec(),
            kinds: wire::KIND_NAMESPACE,
            parent: Some(parent),
            scope: true,
            ..FakeEntity::default()
        }
    }

    /// A function in `parent` with these parameters.
    pub fn function(identifier: &str, parent: Entity, parameters: Vec<Entity>) -> FakeEntity {
        FakeEntity {
            identifier: identifier.as_bytes().to_vec(),
            kinds: wire::KIND_FUNCTION,
            parent: Some(parent),
            parameters: Some(parameters),
            ..FakeEntity::default()
        }
    }

    /// A function parameter of `function`.
    pub fn parameter(identifier: &str, function: Entity) -> FakeEntity {
        FakeEntity {
            identifier: identifier.as_bytes().to_vec(),
            kinds: wire::KIND_PARAMETER,
            parent: Some(function),
            ..FakeEntity::default()
        }
    }

    /// A class in `parent`.
    pub fn class(identifier: &str, parent: Entity) -> FakeEntity {
        FakeEntity {
            identifier: identifier.as_bytes().to_vec(),
            kinds: wire::KIND_TYPE,
            parent: Some(parent),
            scope: true,
            ..FakeEntity::default()
        }
    }
}

/// A written declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeDeclaration {
    pub entity: Entity,
    /// `name_range_of`.
    pub name: Option<Range>,
    pub definition: bool,
}

/// A written reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeReference {
    pub referent: Entity,
    /// The identifier written; empty for a name that is not one.
    pub identifier: Vec<u8>,
    /// `name_range_of`.
    pub name: Option<Range>,
    /// `extent_of`.
    pub extent: Option<Range>,
    /// §7.2 rule 1, third condition. Default `true`.
    pub direct: bool,
    /// `Subject::unit_keys`. Default `Err(Unsupported)`.
    pub unit_keys: Result<Vec<Digest>, Errc>,
}

impl FakeReference {
    /// A direct reference to `referent` written `identifier`, with no range.
    pub fn to(referent: Entity, identifier: &str) -> FakeReference {
        FakeReference {
            referent,
            identifier: identifier.as_bytes().to_vec(),
            name: None,
            extent: None,
            direct: true,
            unit_keys: Err(Errc::Unsupported),
        }
    }
}

/// The call part of an expression (§6.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeCall {
    pub callee: Entity,
    pub reference: Reference,
    pub arguments: Vec<Expression>,
}

/// A written expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeExpression {
    /// `extent_of`.
    pub extent: Option<Range>,
    /// `Some`: the expression is a call.
    pub call: Option<FakeCall>,
    /// `Subject::unit_keys`. Default `Err(Unsupported)`.
    pub unit_keys: Result<Vec<Digest>, Errc>,
}

impl FakeExpression {
    /// An expression that is not a call, written at `extent`.
    pub fn at(extent: Option<Range>) -> FakeExpression {
        FakeExpression { extent, call: None, unit_keys: Err(Errc::Unsupported) }
    }
}

/// A subject unit described literally. `Entity(0)` is the global
/// namespace; files are numbered from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeSubject {
    pub files: Vec<FakeFile>,
    pub entities: Vec<FakeEntity>,
    pub declarations: Vec<FakeDeclaration>,
    pub references: Vec<FakeReference>,
    pub expressions: Vec<FakeExpression>,
    /// Ranges inside a macro argument (§5.4 tier 1): a range given out that
    /// lies in one of these has tier 1.
    pub tier1: Vec<Range>,
    /// `Subject::place` for a value with no place of its own: the nearest
    /// place the host can name (§7.4 rule 3).
    pub nearest: Vec<(Value, Range)>,
    /// `Subject::unresolved_named` (§6.5 rule 6): the count of unresolved
    /// names for each identifier, 0 for one not listed; `None`: the subject
    /// cannot tell. Default `Some` of an empty map.
    pub unresolved: Option<std::collections::BTreeMap<Vec<u8>, u64>>,
}

impl Default for FakeSubject {
    fn default() -> FakeSubject {
        FakeSubject::new()
    }
}

impl FakeSubject {
    /// The global namespace.
    pub const GLOBAL: Entity = Entity(0);

    /// A subject with the global namespace and nothing else.
    pub fn new() -> FakeSubject {
        FakeSubject {
            files: Vec::new(),
            entities: vec![FakeEntity {
                display: "the global namespace".to_owned(),
                kinds: wire::KIND_NAMESPACE,
                scope: true,
                ..FakeEntity::default()
            }],
            declarations: Vec::new(),
            references: Vec::new(),
            expressions: Vec::new(),
            tier1: Vec::new(),
            nearest: Vec::new(),
            unresolved: Some(std::collections::BTreeMap::new()),
        }
    }

    pub fn file(&mut self, path: &str, text: &str, owned: bool) -> File {
        self.files.push(FakeFile {
            path: path.as_bytes().to_vec(),
            text: text.as_bytes().to_vec(),
            owned,
        });
        File(self.files.len() as u32)
    }

    pub fn entity(&mut self, e: FakeEntity) -> Entity {
        self.entities.push(e);
        Entity(self.entities.len() as u32 - 1)
    }

    pub fn declaration(&mut self, d: FakeDeclaration) -> Declaration {
        self.declarations.push(d);
        Declaration(self.declarations.len() as u32 - 1)
    }

    pub fn reference(&mut self, r: FakeReference) -> Reference {
        self.references.push(r);
        Reference(self.references.len() as u32 - 1)
    }

    pub fn expression(&mut self, x: FakeExpression) -> Expression {
        self.expressions.push(x);
        Expression(self.expressions.len() as u32 - 1)
    }

    /// The range of the `nth` (from 0) occurrence of `needle` in file `f`.
    /// A test helper: panics if there is none.
    pub fn range_nth(&self, f: File, needle: &str, nth: usize) -> Range {
        let text = &self.file_of(f).expect("a file of the fake subject").text;
        let needle = needle.as_bytes();
        let lo = (0..=text.len().saturating_sub(needle.len()))
            .filter(|&i| text[i..].starts_with(needle))
            .nth(nth)
            .unwrap_or_else(|| panic!("occurrence {nth} of {needle:?} in the fake file"));
        Range { file: f, lo: lo as u32, hi: (lo + needle.len()) as u32 }
    }

    /// The range of the first occurrence of `needle` in file `f`.
    pub fn range(&self, f: File, needle: &str) -> Range {
        self.range_nth(f, needle, 0)
    }

    fn file_of(&self, f: File) -> Option<&FakeFile> {
        (f.0 as usize).checked_sub(1).and_then(|i| self.files.get(i))
    }

    fn ent(&self, e: Entity) -> &FakeEntity {
        &self.entities[e.0 as usize]
    }

    fn is(&self, e: Entity, kind: u64) -> bool {
        self.ent(e).kinds & kind != 0
    }

    fn first_declaration(&self, e: Entity) -> Option<&FakeDeclaration> {
        self.declarations.iter().find(|d| d.entity == e)
    }

    /// Whether `lookup` finds `e` (§6.1 rule 5): written, not a
    /// specialisation, not in a function body, not a parameter.
    fn findable(&self, e: Entity) -> bool {
        let fe = self.ent(e);
        fe.origin.is_none()
            && self.first_declaration(e).is_some()
            && fe.parent.is_some_and(|p| !self.is(p, wire::KIND_FUNCTION))
            && !self.is(e, wire::KIND_PARAMETER)
    }

    /// Whether `e` is a member of `scope` for `lookup`: its parent is
    /// `scope` or a transparent member of it (§6.1 rule 3).
    fn member_of(&self, e: Entity, scope: Entity) -> bool {
        let mut p = self.ent(e).parent;
        while let Some(q) = p {
            if q == scope {
                return true;
            }
            if !self.ent(q).transparent {
                return false;
            }
            p = self.ent(q).parent;
        }
        false
    }

    /// The ranges the fake gives out (§5.4).
    fn given_out(&self) -> impl Iterator<Item = Range> + '_ {
        self.declarations
            .iter()
            .filter_map(|d| d.name)
            .chain(self.references.iter().flat_map(|r| [r.name, r.extent]).flatten())
            .chain(self.expressions.iter().filter_map(|x| x.extent))
    }
}

fn within(inner: Range, outer: Range) -> bool {
    inner.file == outer.file && outer.lo <= inner.lo && inner.hi <= outer.hi
}

impl Subject for FakeSubject {
    fn is_live(&self, v: Value) -> bool {
        let n = v.number() as usize;
        match v {
            Value::Entity(_) => n < self.entities.len(),
            Value::Declaration(_) => n < self.declarations.len(),
            Value::Reference(_) => n < self.references.len(),
            Value::Expression(_) => n < self.expressions.len(),
        }
    }

    fn file_len(&self, f: File) -> Option<u32> {
        self.file_of(f).map(|f| f.text.len() as u32)
    }

    fn kind_bits(&self, v: Value) -> u64 {
        v.sort_bit()
            | match v {
                Value::Entity(e) => self.ent(e).kinds & wire::KIND_ENTITY_BITS,
                Value::Declaration(d) => {
                    if self.declarations[d.0 as usize].definition {
                        wire::KIND_DEFINITION
                    } else {
                        0
                    }
                }
                Value::Reference(_) => 0,
                Value::Expression(x) => {
                    if self.expressions[x.0 as usize].call.is_some() {
                        wire::KIND_CALL
                    } else {
                        0
                    }
                }
            }
    }

    fn lookup(&self, _global: bool, components: &[&[u8]]) -> Vec<Entity> {
        let mut scopes = vec![FakeSubject::GLOBAL];
        let mut found = vec![FakeSubject::GLOBAL];
        for c in components {
            found = (0..self.entities.len() as u32)
                .map(Entity)
                .filter(|&e| {
                    self.ent(e).identifier == *c
                        && self.findable(e)
                        && scopes.iter().any(|&s| self.member_of(e, s))
                })
                .collect();
            scopes = found.iter().copied().filter(|&e| self.ent(e).scope).collect();
        }
        // Unit order of first declarations.
        found.sort_by_key(|&e| {
            self.declarations.iter().position(|d| d.entity == e).unwrap_or(usize::MAX)
        });
        found
    }

    fn identifier(&self, v: Value) -> Option<Vec<u8>> {
        match v {
            Value::Entity(e) => Some(self.ent(e).identifier.clone()),
            Value::Declaration(d) => Some(self.ent(self.entity_of(d)).identifier.clone()),
            Value::Reference(r) => Some(self.references[r.0 as usize].identifier.clone()),
            Value::Expression(_) => None,
        }
    }

    fn display(&self, v: Value) -> String {
        let name = |e: Entity| {
            let fe = self.ent(e);
            if fe.display.is_empty() {
                String::from_utf8_lossy(&fe.identifier).into_owned()
            } else {
                fe.display.clone()
            }
        };
        match v {
            Value::Entity(e) => name(e),
            Value::Declaration(d) => format!("a declaration of {}", name(self.entity_of(d))),
            Value::Reference(r) => format!("a reference to {}", name(self.referent(r))),
            Value::Expression(x) => format!("expression {}", x.0),
        }
    }

    fn parent(&self, e: Entity) -> Option<Entity> {
        self.ent(e).parent
    }

    fn parameters(&self, v: Value) -> Option<Vec<Entity>> {
        match v {
            Value::Entity(e) => self.ent(e).parameters.clone(),
            Value::Declaration(d) => self.ent(self.entity_of(d)).parameters.clone(),
            Value::Reference(_) | Value::Expression(_) => None,
        }
    }

    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc> {
        if self.is(e, wire::KIND_PARAMETER) {
            return Err(Errc::Unsupported);
        }
        Ok((0..self.declarations.len() as u32)
            .map(Declaration)
            .filter(|d| self.declarations[d.0 as usize].entity == e)
            .collect())
    }

    fn entity_of(&self, d: Declaration) -> Entity {
        self.declarations[d.0 as usize].entity
    }

    fn origin(&self, e: Entity) -> Entity {
        // Robust to a referent that is not live, so that tests can describe
        // an inconsistent subject.
        self.entities.get(e.0 as usize).and_then(|x| x.origin).unwrap_or(e)
    }

    fn references_to(&self, e: Entity) -> Vec<Reference> {
        (0..self.references.len() as u32)
            .map(Reference)
            .filter(|&r| self.origin(self.referent(r)) == e)
            .collect()
    }

    fn referent(&self, r: Reference) -> Entity {
        self.references[r.0 as usize].referent
    }

    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc> {
        match v {
            Value::Declaration(d) => Ok(self.declarations[d.0 as usize].name),
            Value::Reference(r) => Ok(self.references[r.0 as usize].name),
            Value::Entity(_) | Value::Expression(_) => Err(Errc::WrongKind),
        }
    }

    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        match v {
            Value::Reference(r) => Ok(self.references[r.0 as usize].extent),
            Value::Expression(x) => Ok(self.expressions[x.0 as usize].extent),
            Value::Entity(_) => Err(Errc::WrongKind),
            Value::Declaration(_) => Err(Errc::Unsupported),
        }
    }

    fn text(&self, r: Range) -> &[u8] {
        &self.file_of(r.file).expect("a live file").text[r.lo as usize..r.hi as usize]
    }

    fn owned(&self, f: File) -> bool {
        self.file_of(f).is_some_and(|f| f.owned)
    }

    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc> {
        if !self.is(e, wire::KIND_FUNCTION) && self.ent(e).parameters.is_none() {
            return Err(Errc::WrongKind);
        }
        Ok((0..self.expressions.len() as u32)
            .map(Expression)
            .filter(|&x| self.callee(x).is_some_and(|c| self.origin(c) == e))
            .collect())
    }

    fn callee(&self, x: Expression) -> Option<Entity> {
        self.expressions[x.0 as usize].call.as_ref().map(|c| c.callee)
    }

    fn callee_reference(&self, x: Expression) -> Option<Reference> {
        self.expressions[x.0 as usize].call.as_ref().map(|c| c.reference)
    }

    fn arguments(&self, x: Expression) -> Option<Vec<Expression>> {
        self.expressions[x.0 as usize].call.as_ref().map(|c| c.arguments.clone())
    }

    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64> {
        self.unresolved.as_ref().map(|m| m.get(identifier).copied().unwrap_or(0))
    }

    fn tier(&self, r: Range) -> Option<u8> {
        if !self.given_out().any(|g| within(r, g)) {
            return None;
        }
        Some(u8::from(self.tier1.iter().any(|&t| within(r, t))))
    }

    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        let fe = self.ent(e);
        if fe.identifier.is_empty() || fe.origin.is_some() || self.is(e, wire::KIND_PARAMETER) {
            return None;
        }
        let mut names = vec![fe.identifier.clone()];
        let mut p = fe.parent?;
        while let Some(grand) = self.ent(p).parent {
            let fp = self.ent(p);
            let namespace = fp.kinds & wire::KIND_NAMESPACE != 0;
            if fp.kinds & (wire::KIND_TEMPLATE | wire::KIND_FUNCTION) != 0 || fp.origin.is_some() {
                return None;
            }
            match (fp.identifier.is_empty(), namespace) {
                (true, true) => {}
                (true, false) => return None,
                (false, _) => names.push(fp.identifier.clone()),
            }
            p = grand;
        }
        names.reverse();
        Some(names)
    }

    fn references_at(&self, r: Range) -> Vec<Reference> {
        (0..self.references.len() as u32)
            .map(Reference)
            .filter(|x| self.references[x.0 as usize].extent == Some(r))
            .collect()
    }

    fn found_directly(&self, r: Reference) -> bool {
        self.references[r.0 as usize].direct
    }

    fn entity_key(&self, e: Entity) -> Option<Digest> {
        self.ent(e).key
    }

    fn unit_keys(&self, v: Value) -> Result<Vec<Digest>, Errc> {
        match v {
            Value::Reference(r) => self.references[r.0 as usize].unit_keys.clone(),
            Value::Expression(x) => self.expressions[x.0 as usize].unit_keys.clone(),
            Value::Entity(_) | Value::Declaration(_) => Err(Errc::Unsupported),
        }
    }

    fn rename_rule(&self, e: Entity, new_identifier: &[u8]) -> Result<Rule, Errc> {
        let fe = self.ent(e);
        let Some((parent, kind, disc)) = fe.rule else { return Err(Errc::Unsupported) };
        // The rule that undoes the rename (`spec/verify.md` §2.1): it
        // matches the new name and gives back the old one.
        Ok(Rule {
            parent,
            kind,
            name: Name::Identifier(new_identifier.to_vec()),
            disc,
            to_parent: parent,
            to_name: Name::Identifier(fe.identifier.clone()),
        })
    }

    fn place(&self, v: Value) -> Option<(Range, bool)> {
        let own = match v {
            Value::Entity(e) => self.first_declaration(e).and_then(|d| d.name),
            Value::Declaration(d) => self.declarations[d.0 as usize].name,
            Value::Reference(r) => self.references[r.0 as usize].extent,
            Value::Expression(x) => self.expressions[x.0 as usize].extent,
        };
        match own {
            Some(r) => Some((r, true)),
            None => self.nearest.iter().find(|(w, _)| *w == v).map(|&(_, r)| (r, false)),
        }
    }

    fn file_ref(&self, f: File) -> FileRef {
        let file = self.file_of(f).expect("a live file");
        FileRef {
            path: file.path.clone(),
            size: file.text.len() as u64,
            content: slip_model::hash::content_digest(&file.text),
        }
    }
}

/// An [`Out`] over two vectors with fixed capacities, that remembers what
/// was written. For tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FakeOut {
    pub int_capacity: usize,
    pub byte_capacity: usize,
    pub ints: Vec<u64>,
    pub bytes: Vec<u8>,
    /// How many times a write method was called.
    pub writes: usize,
}

impl FakeOut {
    pub fn with_capacity(ints: usize, bytes: usize) -> FakeOut {
        FakeOut { int_capacity: ints, byte_capacity: bytes, ..FakeOut::default() }
    }
}

impl Out for FakeOut {
    fn int_capacity(&self) -> usize {
        self.int_capacity
    }

    fn byte_capacity(&self) -> usize {
        self.byte_capacity
    }

    fn write_ints(&mut self, v: &[u64]) {
        assert!(v.len() <= self.int_capacity, "the host wrote past the integer capacity");
        self.writes += 1;
        self.ints = v.to_vec();
    }

    fn write_bytes(&mut self, v: &[u8]) {
        assert!(v.len() <= self.byte_capacity, "the host wrote past the byte capacity");
        self.writes += 1;
        self.bytes = v.to_vec();
    }
}
