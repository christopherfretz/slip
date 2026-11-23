//! The effects (`spec/api.md` §7): operations 64 to 84.
//!
//! An effect is staged like a query (`host.rs`): [`Host::effect`] reads
//! the arguments, checks every refusal of §7 and every limit of §7.7 that
//! the request reaches, asks the subject what the rule needs and checks
//! its answers, and builds the whole record. It changes nothing.
//! [`Host::record`] then stores it, and nothing it does can fail. So a
//! refused request, and a request the subject answered inconsistently,
//! records nothing (§4 rule 3, §3.5 rule 2).
//!
//! The host judges each request alone: it never compares one with
//! another (§7.3 rule 3). Joining identical requests and merging groups
//! is `RunOutput::canonical`'s, at `Host::finish`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use slip_model::keys::{Name, Rule};
use slip_model::{Digest, FileRef};

use crate::host::{
    ALLOWED_EXTENT, ALLOWED_NAME_RANGE, ALLOWED_RENAME_RULE, ALLOWED_UNIT_KEYS, Answer, Host,
    Record, Step, Stop, check_identifier, refuse,
};
use crate::output::{Changed, Edit, Piece, Place, Report, Severity, Tiers};
use crate::subject::{Declaration, Entity, File, PlaceOf, Range, Reference, Value};
use crate::wire::{self, Errc, Operation};

/// A piece of a code value as the host holds it (§7.1). A spelled piece
/// is not text yet: its text is chosen when an edit is recorded (§7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeldPiece {
    /// Never empty: an empty literal is no piece, and adjacent literals
    /// are one (§7.1 rule 6).
    Literal(Vec<u8>),
    Copy(Range),
    /// One identifier, checked (§7.1, `identifier`): a name given as data.
    /// Never merged with a literal or another identifier.
    Identifier(Vec<u8>),
    Spelled {
        entity: Entity,
        key: Digest,
        /// The fully qualified text of §7.2 rule 2 (`::a::b::e`).
        qualified: Vec<u8>,
        /// Where the entity's identifier begins in `qualified`.
        identifier_at: usize,
    },
}

impl HeldPiece {
    /// The bytes of text it holds, for the limit of §7.7.
    fn text_len(&self) -> u64 {
        match self {
            HeldPiece::Literal(b) | HeldPiece::Identifier(b) => b.len() as u64,
            HeldPiece::Copy(_) => 0,
            HeldPiece::Spelled { qualified, .. } => qualified.len() as u64,
        }
    }
}

/// A code value as the host holds it: its pieces and the bytes of text
/// they hold, so that a request can be judged against the limits from
/// two numbers before anything is copied.
#[derive(Clone, Debug, Default)]
pub(crate) struct Code {
    pieces: Vec<HeldPiece>,
    bytes: u64,
}

impl Code {
    fn new(pieces: Vec<HeldPiece>) -> Code {
        let bytes = pieces.iter().map(HeldPiece::text_len).sum();
        Code { pieces, bytes }
    }
}

/// The one piece an edit narrowed to a name range records (§7.2 rule 1):
/// a sibling's spelling, or, at a declaration's name, its identifier.
enum Narrowed<'c> {
    Spelled(Digest, &'c [u8]),
    Identifier(&'c [u8]),
}

impl Narrowed<'_> {
    fn text(&self) -> &[u8] {
        match self {
            Narrowed::Spelled(_, t) | Narrowed::Identifier(t) => t,
        }
    }
}

/// The empty code: the null handle.
static EMPTY: Code = Code { pieces: Vec::new(), bytes: 0 };

/// The origins of the referents named at a name range, each with its key
/// (§7.2 rule 6).
pub(crate) type Named = Vec<(Entity, Option<Digest>)>;

/// What the host holds for the effects of a run, besides the record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Effects {
    /// The code values made, by handle index (§3.4 tag 5). Each has at
    /// least one piece: a code with none is the null handle.
    codes: Vec<Code>,
    /// The identity of each file a place names, made once per file so
    /// that every place shares it (§7.7: a run holds each path once). A
    /// memo of `Subject::file_ref`, which is a function of the file: it
    /// does not change what a request records, so the validation stage may
    /// fill it.
    files: RefCell<BTreeMap<File, Arc<FileRef>>>,
    /// Pieces held in code values and recorded edits (§7.7).
    pieces: u64,
    /// Bytes of text held in pieces of code values and recorded edits.
    piece_bytes: u64,
    /// Bytes of text arguments held: notes, messages, reasons, names.
    text: u64,
    /// The identifier each entity was declared renamed to (§7.5 rule 3).
    pub(crate) renamed: BTreeMap<Entity, Vec<u8>>,
    /// For each recorded edit whose replacement is one identifier piece at
    /// the name range of references: its index, and the origin of each
    /// reference's referent with that origin's key (§7.2 rule 6). The
    /// piece takes a key in `Host::finish`, from what was declared
    /// renamed by then, so that the order of requests does not matter.
    pub(crate) named: Vec<(usize, Named)>,
    /// What the run asked for, raw.
    pub(crate) record: Record,
}

/// A validated effect: what [`Host::record`] stores, and what it costs.
#[derive(Debug)]
pub(crate) enum Effect {
    /// A new code value (not empty).
    Code(Code),
    Group,
    Edit {
        edit: Edit,
        group: Option<usize>,
        pieces: u64,
        bytes: u64,
        /// The referents' origins of an identifier piece put at a name
        /// range (`Effects::named`); empty otherwise.
        named: Named,
    },
    Report(Report),
    /// The declared changes not already recorded.
    Changed(Vec<Changed>),
    Renamed {
        entity: Entity,
        name: Vec<u8>,
        rule: Rule,
    },
    /// A request that repeats one already recorded (§7.5 rule 4).
    Nothing,
}

fn limit<T>(what: &str, limit: u64) -> Step<T> {
    refuse(Errc::LimitExceeded, format!("{what}: the limit of a run is {limit} (§7.7)"))
}

/// Whether `inner` lies in `outer`.
fn within(inner: Range, outer: Range) -> bool {
    inner.file == outer.file && outer.lo <= inner.lo && inner.hi <= outer.hi
}

impl Host<'_> {
    /// Operations 64 to 84: the checks and the record to make. Changes
    /// nothing.
    pub(crate) fn effect(&self, row: &Operation, ints: &[u64], bytes: &[u8]) -> Step<Answer> {
        let effect = match row.op {
            wire::OP_CODE_TEXT => {
                if bytes.is_empty() {
                    return Ok(Answer::Word(0));
                }
                self.code_fits(1, bytes.len() as u64)?;
                Effect::Code(Code::new(vec![HeldPiece::Literal(bytes.to_vec())]))
            }
            wire::OP_CODE_SPELL => self.spell(ints[0])?,
            wire::OP_CODE_IDENTIFIER => {
                self.identifier_arg(bytes, "the name of an identifier piece")?;
                self.one_piece(HeldPiece::Identifier(bytes.to_vec()))?
            }
            wire::OP_CODE_COPY => {
                let r = self.range_arg(ints[0], ints[1])?;
                // §7.1 rule 2: the bytes of an empty range are none, so its
                // copy is the empty code, as `text("")` is: no piece.
                if r.lo == r.hi {
                    return Ok(Answer::Word(0));
                }
                self.one_piece(HeldPiece::Copy(r))?
            }
            wire::OP_CODE_CAT => match self.cat(ints)? {
                Some(e) => e,
                None => return Ok(Answer::Word(0)),
            },
            wire::OP_REPLACE => self.replace(ints, bytes)?,
            wire::OP_NEW_GROUP => {
                if self.effects.record.groups.len() as u64 >= self.limits.groups {
                    return limit("groups", self.limits.groups);
                }
                Effect::Group
            }
            wire::OP_REPORT => self.report(ints, bytes)?,
            wire::OP_EXPECT_CHANGED => self.expect_changed(ints[0], bytes)?,
            wire::OP_EXPECT_RENAMED => self.expect_renamed(ints[0], bytes)?,
            _ => unreachable!("operation {} is an effect of the table and not dispatched", row.op),
        };
        Ok(Answer::Effect(effect))
    }

    /// Stores a validated effect and returns the value of its result
    /// word: the handle of a code or a group, else 0.
    pub(crate) fn record(&mut self, effect: Effect) -> u64 {
        let fx = &mut self.effects;
        match effect {
            Effect::Code(code) => {
                fx.pieces += code.pieces.len() as u64;
                fx.piece_bytes += code.bytes;
                fx.codes.push(code);
                wire::handle(wire::TAG_CODE, fx.codes.len() as u64 - 1)
            }
            Effect::Group => {
                fx.record.groups.push(Vec::new());
                wire::handle(wire::TAG_GROUP, fx.record.groups.len() as u64 - 1)
            }
            Effect::Edit { edit, group, pieces, bytes, named } => {
                fx.pieces += pieces;
                fx.piece_bytes += bytes;
                fx.text += edit.notes.iter().map(|n| n.len() as u64).sum::<u64>();
                fx.record.edits.push(edit);
                let index = fx.record.edits.len() - 1;
                if !named.is_empty() {
                    fx.named.push((index, named));
                }
                if let Some(g) = group
                    && let Some(members) = fx.record.groups.get_mut(g)
                {
                    members.push(index);
                }
                0
            }
            Effect::Report(r) => {
                fx.text += r.message.len() as u64;
                fx.record.reports.push(r);
                0
            }
            Effect::Changed(cs) => {
                for c in cs {
                    fx.text += c.reason.len() as u64;
                    fx.record.changed.insert(c);
                }
                0
            }
            Effect::Renamed { entity, name, rule } => {
                fx.text += name.len() as u64;
                fx.renamed.insert(entity, name);
                fx.record.renamed.push(rule);
                0
            }
            Effect::Nothing => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Arguments.

    /// The code of handle `h`; the null handle is the empty code.
    fn code_arg(&self, h: u64) -> Step<&Code> {
        if h == 0 {
            return Ok(&EMPTY);
        }
        if wire::handle_tag(h) != Some(wire::TAG_CODE) {
            return refuse(Errc::InvalidHandle, "not the handle of a code");
        }
        match usize::try_from(wire::handle_index(h)).ok().and_then(|i| self.effects.codes.get(i)) {
            Some(p) => Ok(p),
            None => refuse(Errc::InvalidHandle, "a code this run did not make"),
        }
    }

    /// The group of handle `h`; the null handle is no group.
    fn group_arg(&self, h: u64) -> Step<Option<usize>> {
        if h == 0 {
            return Ok(None);
        }
        if wire::handle_tag(h) != Some(wire::TAG_GROUP) {
            return refuse(Errc::InvalidHandle, "not the handle of a group");
        }
        match usize::try_from(wire::handle_index(h)) {
            Ok(i) if i < self.effects.record.groups.len() => Ok(Some(i)),
            _ => refuse(Errc::InvalidHandle, "a group this run did not make"),
        }
    }

    /// A note, reason or message: at most `text_bytes` bytes (§7.7).
    fn text_arg(&self, text: &[u8], what: &str) -> Step<()> {
        if text.len() as u64 > self.limits.text_bytes {
            return limit(&format!("{what} of {} bytes", text.len()), self.limits.text_bytes);
        }
        Ok(())
    }

    /// Whether `n` more bytes of text arguments fit (§7.7).
    fn text_fits(&self, n: u64) -> Step<()> {
        if self.effects.text.saturating_add(n) > self.limits.text_total {
            return limit("bytes of notes, reasons, messages and names", self.limits.text_total);
        }
        Ok(())
    }

    /// Whether `pieces` more pieces holding `bytes` more bytes fit (§7.7).
    fn pieces_fit(&self, pieces: u64, bytes: u64) -> Step<()> {
        if self.effects.pieces.saturating_add(pieces) > self.limits.pieces {
            return limit("pieces held", self.limits.pieces);
        }
        if self.effects.piece_bytes.saturating_add(bytes) > self.limits.literal_bytes {
            return limit("bytes of text in pieces", self.limits.literal_bytes);
        }
        Ok(())
    }

    /// The place of a range the host checked (§7.6 `(file, lo, hi)`).
    fn place_of(&self, r: Range) -> Place {
        let file = Arc::clone(
            self.effects
                .files
                .borrow_mut()
                .entry(r.file)
                .or_insert_with(|| Arc::new(self.subject.file_ref(r.file))),
        );
        Place { file, lo: r.lo, hi: r.hi }
    }

    /// A range the subject gave: a host failure unless it lies in its file.
    fn subject_range(&self, method: &str, r: Range) -> Step<Range> {
        if self.range_is_valid(r) {
            Ok(r)
        } else {
            Err(Stop::Internal(format!("Subject::{method} gave a range outside its file")))
        }
    }

    /// A value the subject gave: a host failure unless it is live.
    fn subject_value(&self, method: &str, v: Value) -> Step<Value> {
        if self.subject.is_live(v) {
            Ok(v)
        } else {
            Err(Stop::Internal(format!(
                "Subject::{method} gave {} that is not live",
                v.sort_name()
            )))
        }
    }

    /// `Subject::tiers` of `r`: a set that is not empty, `None` when `r` is
    /// not exactly a range a query gives out; the empty set is a host
    /// failure.
    fn tiers(&self, r: Range) -> Step<Option<Tiers>> {
        match self.subject.tiers(r) {
            None => Ok(None),
            Some(reached) => Tiers::from_reached(reached).map(Some).ok_or_else(|| {
                Stop::Internal(
                    "Subject::tiers gave the empty set; a range given out has a tier".to_owned(),
                )
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Code values (§7.1).

    /// Whether a new code value of `count` pieces holding `bytes` bytes of
    /// text fits the limits (§7.7). Asked before the code is built, so a
    /// refused request copies nothing.
    fn code_fits(&self, count: u64, bytes: u64) -> Step<()> {
        if self.effects.codes.len() as u64 >= self.limits.code_values {
            return limit("code values", self.limits.code_values);
        }
        if count > self.limits.pieces_per_code {
            return limit("pieces in one code value", self.limits.pieces_per_code);
        }
        self.pieces_fit(count, bytes)
    }

    /// A code value of one piece.
    fn one_piece(&self, p: HeldPiece) -> Step<Effect> {
        self.code_fits(1, p.text_len())?;
        Ok(Effect::Code(Code::new(vec![p])))
    }

    /// §7.1 rule 4: `spell(e)`. The key is asked here (decision 8 of
    /// WP-088), the text is chosen at `replace`.
    fn spell(&self, h: u64) -> Step<Effect> {
        let e = self.entity_arg(h)?;
        let v = Value::Entity(e);
        let Some(names) = self.subject.qualified(e) else {
            return Err(self.refusal_at(
                v,
                Errc::Unsupported,
                "the entity cannot be named by a qualified name without template arguments \
                 (§7.1 rule 4)"
                    .to_owned(),
            ));
        };
        // What the subject answered: one identifier or more, none empty,
        // the last the entity's own.
        let own = self.subject.identifier(v).unwrap_or_default();
        if names.is_empty() || names.iter().any(Vec::is_empty) || names.last() != Some(&own) {
            return Err(Stop::Internal(format!(
                "Subject::qualified gave {} names, not ending in the entity's identifier",
                names.len()
            )));
        }
        let Some(key) = self.subject.entity_key(e) else {
            return Err(self.refusal_at(
                v,
                Errc::Unsupported,
                "the digest has no key for the entity, so its spelling could not be checked \
                 (§7.1 rule 4)"
                    .to_owned(),
            ));
        };
        let len: u64 = names.iter().map(|n| n.len() as u64 + 2).sum();
        self.code_fits(1, len)?;
        let mut qualified = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        let mut identifier_at = 0;
        for n in &names {
            qualified.extend_from_slice(b"::");
            identifier_at = qualified.len();
            qualified.extend_from_slice(n);
        }
        let piece = HeldPiece::Spelled { entity: e, key, qualified, identifier_at };
        Ok(Effect::Code(Code::new(vec![piece])))
    }

    /// §7.1 rule 5 at L0: the pieces of the codes in order, adjacent
    /// literals joined. `None`: the result is the empty code. The size of
    /// the result is computed from the codes' counts first, in time
    /// proportional to the number of arguments, and checked; only then is
    /// anything copied.
    fn cat(&self, handles: &[u64]) -> Step<Option<Effect>> {
        let mut codes = Vec::with_capacity(handles.len().min(1 << 16));
        for &h in handles {
            codes.push(self.code_arg(h)?);
        }
        let is_literal = |p: Option<&HeldPiece>| matches!(p, Some(HeldPiece::Literal(_)));
        let (mut count, mut bytes, mut last_literal) = (0u64, 0u64, false);
        for c in &codes {
            let Some(first) = c.pieces.first() else { continue };
            count += c.pieces.len() as u64;
            if last_literal && is_literal(Some(first)) {
                count -= 1;
            }
            last_literal = is_literal(c.pieces.last());
            bytes = bytes.saturating_add(c.bytes);
        }
        if count == 0 {
            return Ok(None);
        }
        self.code_fits(count, bytes)?;
        let mut out: Vec<HeldPiece> = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for p in codes.into_iter().flat_map(|c| &c.pieces) {
            if let (Some(HeldPiece::Literal(prev)), HeldPiece::Literal(b)) = (out.last_mut(), p) {
                prev.extend_from_slice(b);
                continue;
            }
            out.push(p.clone());
        }
        Ok(Some(Effect::Code(Code { pieces: out, bytes })))
    }

    // -----------------------------------------------------------------------
    // `replace` and the spelling (§7.2, §7.3).

    fn replace(&self, ints: &[u64], note: &[u8]) -> Step<Effect> {
        let r = self.range_arg(ints[0], ints[1])?;
        let code = self.code_arg(ints[2])?;
        let group = self.group_arg(ints[3])?;
        self.text_arg(note, "a note")?;
        if r.lo == r.hi {
            return refuse(
                Errc::InvalidArgument,
                "an empty range: insertion is not in version 0 (§7.3 rule 2)",
            );
        }
        if !self.subject.owned(r.file) {
            return refuse(Errc::NotEditable, "a range in a file this run may not edit");
        }
        let Some(tiers) = self.tiers(r)? else {
            return refuse(
                Errc::InvalidHandle,
                "a range that is not one name_range_of or extent_of returns in the subject unit \
                 (§7.3 rule 2, §5.2 rule 2)",
            );
        };
        if self.effects.record.edits.len() as u64 >= self.limits.edits {
            return limit("edits requested", self.limits.edits);
        }
        // §7.2: the sibling rule, or every spelled piece fully qualified.
        let narrowed = match code.pieces.as_slice() {
            [HeldPiece::Spelled { entity, key, qualified, identifier_at }] => {
                match self.sibling(r, *entity)? {
                    Some(name) => {
                        let Some(t) = self.tiers(name)? else {
                            return Err(Stop::Internal(
                                "Subject::tiers gave None for a name range it gave out".to_owned(),
                            ));
                        };
                        let text = qualified.get(*identifier_at..).unwrap_or_default();
                        Some((name, t, Narrowed::Spelled(*key, text)))
                    }
                    None => None,
                }
            }
            _ => None,
        };
        // §7.2 rule 1 at a name range that is no reference's extent: a
        // sibling's identifier, or refused (`spec/landed/api/WP-132.md`).
        let narrowed = match narrowed {
            Some(n) => Some(n),
            None => self.at_a_name(r, code)?,
        };
        // The limits, from the counts: nothing is copied before they pass.
        let (count, bytes) = match &narrowed {
            Some((.., n)) => (1, n.text().len() as u64),
            None => (code.pieces.len() as u64, code.bytes),
        };
        self.pieces_fit(count, bytes)?;
        self.text_fits(note.len() as u64)?;
        // §7.2 rule 6: an identifier piece that is the whole replacement
        // of the name range of references may take the key of their
        // referent in `Host::finish`.
        let named = match code.pieces.as_slice() {
            [HeldPiece::Identifier(_)] if narrowed.is_none() => {
                let mut named = Vec::new();
                for x in self.references_named(r)? {
                    let origin = self.origin_of(x)?;
                    named.push((origin, self.subject.entity_key(origin)));
                }
                named
            }
            _ => Vec::new(),
        };
        let (range, tiers, pieces) = match narrowed {
            Some((name, t, Narrowed::Spelled(key, text))) => {
                (name, t, vec![Piece::Spelled { key, text: text.to_vec() }])
            }
            Some((name, t, Narrowed::Identifier(text))) => {
                (name, t, vec![Piece::Identifier { text: text.to_vec(), key: None }])
            }
            None => (r, tiers, code.pieces.iter().map(|p| self.piece(p)).collect()),
        };
        let notes = if note.is_empty() { Vec::new() } else { vec![note.to_vec()] };
        let edit = Edit { place: self.place_of(range), pieces, tiers, notes };
        Ok(Effect::Edit { pieces: count, bytes, edit, group, named })
    }

    /// A held piece as recorded away from the sibling rule: a spelled
    /// piece takes its fully qualified text (§7.2 rule 2).
    fn piece(&self, p: &HeldPiece) -> Piece {
        match p {
            HeldPiece::Literal(b) => Piece::Literal(b.clone()),
            HeldPiece::Copy(r) => Piece::Copy(self.place_of(*r)),
            HeldPiece::Identifier(b) => Piece::Identifier { text: b.clone(), key: None },
            HeldPiece::Spelled { key, qualified, .. } => {
                Piece::Spelled { key: *key, text: qualified.clone() }
            }
        }
    }

    /// §7.2 rule 1: the name range to edit instead of `r` when the code
    /// is one spelled piece for `e` and every reference whose extent is
    /// `r` meets the four conditions with one name range. `None`: the
    /// rule does not apply.
    fn sibling(&self, r: Range, e: Entity) -> Step<Option<Range>> {
        let subject = self.subject;
        let refs = subject.references_at(r);
        if refs.is_empty() {
            return Ok(None);
        }
        let parent = subject.parent(e);
        if let Some(p) = parent {
            self.subject_value("parent", Value::Entity(p))?;
        }
        // Condition 1. Always true of an entity `spell` accepted (§7.1 rule
        // 4), and checked so that the rule reads as written.
        let mut holds = subject.identifier(Value::Entity(e)).is_some_and(|id| !id.is_empty());
        let mut name: Option<Range> = None;
        let mut seen = BTreeSet::new();
        for &x in &refs {
            let v = self.subject_value("references_at", Value::Reference(x))?;
            if !seen.insert(x) {
                return Err(Stop::Internal(
                    "Subject::references_at gave a reference twice".to_owned(),
                ));
            }
            match subject.extent(v) {
                Ok(Some(extent)) if extent == r => {}
                Ok(_) => {
                    return Err(Stop::Internal(
                        "Subject::references_at gave a reference whose extent is not the range"
                            .to_owned(),
                    ));
                }
                Err(errc) => return Err(self.subject_status("extent", v, errc, ALLOWED_EXTENT)),
            }
            let origin = self.origin_of(x)?;
            // Condition 2: the same parent as the origin of the referent.
            holds &= subject.parent(origin) == parent;
            // Condition 3: found directly.
            holds &= subject.found_directly(x);
            // Condition 4, and one name range for all (§7.2 rule 1).
            match subject.name_range(v) {
                Ok(Some(found)) => {
                    let found = self.subject_range("name_range", found)?;
                    if found.lo >= found.hi || !within(found, r) {
                        return Err(Stop::Internal(
                            "Subject::name_range gave a range that is empty or not inside the \
                             reference's extent"
                                .to_owned(),
                        ));
                    }
                    holds &= name.is_none_or(|m| m == found);
                    name = Some(found);
                }
                Ok(None) => holds = false,
                Err(errc) => {
                    return Err(self.subject_status("name_range", v, errc, ALLOWED_NAME_RANGE));
                }
            }
        }
        Ok(if holds { name } else { None })
    }

    /// §7.2 rule 1 at a name range (`spec/landed/api/WP-132.md`): for an
    /// edit of `r` that is the extent of no reference and whose code holds
    /// a spelled piece, when `r` is the name range of a reference or of a
    /// declaration. `None`: `r` is neither, and rule 2 holds. Else the
    /// code must be exactly one spelled piece for an entity `E` with an
    /// identifier, and every reference with that name range must meet the
    /// conditions of the sibling form (`E`'s parent is that of the
    /// origin of the referent; found directly), and every declaration
    /// with it must declare an entity whose parent is `E`'s: then the
    /// edit replaces `r` with `E`'s identifier, as the sibling form
    /// records it where a reference is named, and as an identifier piece
    /// with no key where only declarations are (a declaration's name is no
    /// written reference, so a spelled piece there could never be checked:
    /// `spec/verify.md` §10.7). Otherwise `invalid_argument`: a qualified
    /// spelling after the qualifier written is never recorded.
    fn at_a_name<'c>(
        &self,
        r: Range,
        code: &'c Code,
    ) -> Step<Option<(Range, Tiers, Narrowed<'c>)>> {
        if !code.pieces.iter().any(|p| matches!(p, HeldPiece::Spelled { .. })) {
            return Ok(None);
        }
        let subject = self.subject;
        if !subject.references_at(r).is_empty() {
            return Ok(None);
        }
        let refs = self.references_named(r)?;
        let decls = self.declarations_named(r)?;
        if refs.is_empty() && decls.is_empty() {
            return Ok(None);
        }
        let [HeldPiece::Spelled { entity, key, qualified, identifier_at }] = code.pieces.as_slice()
        else {
            return refuse(
                Errc::InvalidArgument,
                "a code with a spelled piece and other pieces at a name range: a name range \
                 takes one spelled piece, a sibling's identifier (§7.2 rule 1)",
            );
        };
        let e = *entity;
        let parent = subject.parent(e);
        if let Some(p) = parent {
            self.subject_value("parent", Value::Entity(p))?;
        }
        let mut holds = subject.identifier(Value::Entity(e)).is_some_and(|id| !id.is_empty());
        for &x in &refs {
            let origin = self.origin_of(x)?;
            holds &= subject.parent(origin) == parent;
            holds &= subject.found_directly(x);
        }
        for &d in &decls {
            let declared = subject.entity_of(d);
            self.subject_value("entity_of", Value::Entity(declared))?;
            holds &= subject.parent(declared) == parent;
        }
        if !holds {
            return refuse(
                Errc::InvalidArgument,
                "a spelling at a name range is a sibling's identifier, and this entity is no \
                 sibling of the name written there: spell it at the reference's extent (§7.2 \
                 rule 1)",
            );
        }
        let Some(t) = self.tiers(r)? else {
            return Err(Stop::Internal(
                "Subject::tiers gave None for a range replace accepted".to_owned(),
            ));
        };
        let text = qualified.get(*identifier_at..).unwrap_or_default();
        Ok(Some(if refs.is_empty() {
            (r, t, Narrowed::Identifier(text))
        } else {
            (r, t, Narrowed::Spelled(*key, text))
        }))
    }

    /// The references whose name range is `r` (`Subject::references_named_at`),
    /// each checked: live, given once, with that name range.
    fn references_named(&self, r: Range) -> Step<Vec<Reference>> {
        let refs = self.subject.references_named_at(r);
        let mut seen = BTreeSet::new();
        for &x in &refs {
            let v = self.subject_value("references_named_at", Value::Reference(x))?;
            if !seen.insert(x) {
                return Err(Stop::Internal(
                    "Subject::references_named_at gave a reference twice".to_owned(),
                ));
            }
            self.named_at(v, r, "references_named_at")?;
        }
        Ok(refs)
    }

    /// The declarations whose name range is `r`
    /// (`Subject::declarations_named_at`), each checked as
    /// [`Self::references_named`] checks references.
    fn declarations_named(&self, r: Range) -> Step<Vec<Declaration>> {
        let decls = self.subject.declarations_named_at(r);
        let mut seen = BTreeSet::new();
        for &d in &decls {
            let v = self.subject_value("declarations_named_at", Value::Declaration(d))?;
            if !seen.insert(d) {
                return Err(Stop::Internal(
                    "Subject::declarations_named_at gave a declaration twice".to_owned(),
                ));
            }
            self.named_at(v, r, "declarations_named_at")?;
        }
        Ok(decls)
    }

    /// That the name range of `v`, which `method` gave for `r`, is `r`.
    fn named_at(&self, v: Value, r: Range, method: &str) -> Step<()> {
        match self.subject.name_range(v) {
            Ok(Some(found)) if found == r => Ok(()),
            Ok(_) => Err(Stop::Internal(format!(
                "Subject::{method} gave a value whose name range is not the range"
            ))),
            Err(errc) => Err(self.subject_status("name_range", v, errc, ALLOWED_NAME_RANGE)),
        }
    }

    /// The origin (§5.5) of the referent of `x`, both checked live.
    fn origin_of(&self, x: Reference) -> Step<Entity> {
        let referent = self.subject.referent(x);
        self.subject_value("referent", Value::Entity(referent))?;
        let origin = self.subject.origin(referent);
        self.subject_value("origin", Value::Entity(origin))?;
        Ok(origin)
    }

    // -----------------------------------------------------------------------
    // Reports (§7.4).

    fn report(&self, ints: &[u64], message: &[u8]) -> Step<Effect> {
        let (place, attached) = match ints[0] {
            0 => {
                if ints[1] != 0 || ints[2] != 0 {
                    return refuse(Errc::InvalidArgument, "where-kind 0 takes no place");
                }
                (None, false)
            }
            1 => {
                if ints[2] != 0 {
                    return refuse(Errc::InvalidArgument, "where-kind 1 takes one value");
                }
                let v = self.value_arg(ints[1])?;
                match self.subject.place(v) {
                    PlaceOf::Own(r) => {
                        (Some(self.place_of(self.subject_range("place", r)?)), false)
                    }
                    PlaceOf::Nearest(r) => {
                        (Some(self.place_of(self.subject_range("place", r)?)), true)
                    }
                    PlaceOf::Unnamed => (None, true),
                    PlaceOf::Nowhere => (None, false),
                }
            }
            2 => (Some(self.place_of(self.range_arg(ints[1], ints[2])?)), false),
            k => return refuse(Errc::InvalidArgument, format!("where-kind {k} is not 0, 1 or 2")),
        };
        let Some(severity) = Severity::from_wire(ints[3]) else {
            return refuse(
                Errc::InvalidArgument,
                format!("severity {} is not one this host knows (§7.4 rule 4)", ints[3]),
            );
        };
        self.text_arg(message, "a message")?;
        if self.effects.record.reports.len() as u64 >= self.limits.reports {
            return limit("reports", self.limits.reports);
        }
        self.text_fits(message.len() as u64)?;
        Ok(Effect::Report(Report { place, severity, message: message.to_vec(), attached }))
    }

    // -----------------------------------------------------------------------
    // Declared changes (§7.5).

    fn expect_changed(&self, h: u64, reason: &[u8]) -> Step<Effect> {
        let v = self.value_arg(h)?;
        self.text_arg(reason, "a reason")?;
        if let Value::Entity(_) | Value::Declaration(_) = v {
            return Err(self.refusal_at(
                v,
                Errc::Unsupported,
                format!(
                    "a declared change needs a place, and {} has none in version 0 (§7.5 rules \
                     2 and 7)",
                    v.sort_name()
                ),
            ));
        }
        let extent = match self.subject.extent(v) {
            Ok(Some(r)) => self.subject_range("extent", r)?,
            Ok(None) => {
                return Err(self.refusal_at(
                    v,
                    Errc::Unsupported,
                    "the extent is null, so the declaration would have no place (§7.5 rule 2)"
                        .to_owned(),
                ));
            }
            Err(errc) => return Err(self.subject_status("extent", v, errc, ALLOWED_EXTENT)),
        };
        let keys = match self.subject.unit_keys(v) {
            Ok(k) => k,
            Err(errc) => return Err(self.subject_status("unit_keys", v, errc, ALLOWED_UNIT_KEYS)),
        };
        if keys.is_empty() {
            return Err(Stop::Internal("Subject::unit_keys gave no key".to_owned()));
        }
        if keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
            return Err(Stop::Internal("Subject::unit_keys gave a key twice".to_owned()));
        }
        let place = self.place_of(extent);
        let new: Vec<Changed> = keys
            .into_iter()
            .map(|key| Changed { key, place: place.clone(), reason: reason.to_vec() })
            .filter(|c| !self.effects.record.changed.contains(c))
            .collect();
        if new.is_empty() {
            return Ok(Effect::Nothing);
        }
        if (self.effects.record.changed.len() + new.len()) as u64 > self.limits.changed {
            return limit("declared changes", self.limits.changed);
        }
        self.text_fits((reason.len() as u64).saturating_mul(new.len() as u64))?;
        Ok(Effect::Changed(new))
    }

    fn expect_renamed(&self, h: u64, name: &[u8]) -> Step<Effect> {
        let e = self.entity_arg(h)?;
        let v = Value::Entity(e);
        if name.len() as u64 > self.limits.name_bytes {
            return limit(
                &format!("an identifier of {} bytes", name.len()),
                self.limits.name_bytes,
            );
        }
        match check_identifier(name) {
            Ok(()) => {}
            Err(Errc::Unsupported) => {
                return refuse(
                    Errc::Unsupported,
                    "an identifier with `$` or outside the basic character set (§6.1 rule 1)",
                );
            }
            Err(errc) => return refuse(errc, "not an identifier (§7.5 rule 3)"),
        }
        if self.subject.identifier(v).is_some_and(|own| own == name) {
            return Err(self.refusal_at(
                v,
                Errc::InvalidArgument,
                "the entity's identifier already: a rename to it is no rename (§7.5 rule 3)"
                    .to_owned(),
            ));
        }
        match self.effects.renamed.get(&e) {
            Some(n) if n == name => return Ok(Effect::Nothing),
            Some(_) => {
                return Err(self.refusal_at(
                    v,
                    Errc::InvalidArgument,
                    "this run already declared a different name for the entity (§7.5 rule 3)"
                        .to_owned(),
                ));
            }
            None => {}
        }
        let rule = match self.subject.rename_rule(e, name) {
            Ok(r) => r,
            Err(errc) => {
                return Err(self.subject_status("rename_rule", v, errc, ALLOWED_RENAME_RULE));
            }
        };
        // The rule undoes the rename: it matches the new name
        // (`spec/verify.md` §2.1).
        if rule.name != Name::Identifier(name.to_vec()) {
            return Err(Stop::Internal(
                "Subject::rename_rule gave a rule that does not match the new identifier"
                    .to_owned(),
            ));
        }
        self.text_fits(name.len() as u64)?;
        Ok(Effect::Renamed { entity: e, name: name.to_vec(), rule })
    }
}
