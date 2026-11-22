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

use std::collections::{BTreeMap, BTreeSet};

use slip_model::Digest;
use slip_model::keys::{Name, Rule};

use crate::host::{
    ALLOWED_EXTENT, ALLOWED_NAME_RANGE, ALLOWED_RENAME_RULE, ALLOWED_UNIT_KEYS, Answer, Host,
    Record, Step, Stop, check_identifier, refuse,
};
use crate::output::{Changed, Edit, Piece, Place, Report, Severity};
use crate::subject::{Entity, Range, Reference, Value};
use crate::wire::{self, Errc, Operation};

/// A piece of a code value as the host holds it (§7.1). A spelled piece
/// is not text yet: its text is chosen when an edit is recorded (§7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeldPiece {
    /// Never empty: an empty literal is no piece, and adjacent literals
    /// are one (§7.1 rule 6).
    Literal(Vec<u8>),
    Copy(Range),
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
            HeldPiece::Literal(b) => b.len() as u64,
            HeldPiece::Copy(_) => 0,
            HeldPiece::Spelled { qualified, .. } => qualified.len() as u64,
        }
    }
}

/// What the host holds for the effects of a run, besides the record.
#[derive(Clone, Debug, Default)]
pub(crate) struct Effects {
    /// The code values made, by handle index (§3.4 tag 5). Each has at
    /// least one piece: a code with none is the null handle.
    codes: Vec<Vec<HeldPiece>>,
    /// Pieces held in code values and recorded edits (§7.7).
    pieces: u64,
    /// Bytes of text held in pieces of code values and recorded edits.
    piece_bytes: u64,
    /// Bytes of text arguments held: notes, messages, reasons, names.
    text: u64,
    /// The identifier each entity was declared renamed to (§7.5 rule 3).
    renamed: BTreeMap<Entity, Vec<u8>>,
    /// What the run asked for, raw.
    pub(crate) record: Record,
}

/// A validated effect: what [`Host::record`] stores, and what it costs.
#[derive(Debug)]
pub(crate) enum Effect {
    /// A new code value (not empty).
    Code(Vec<HeldPiece>),
    Group,
    Edit {
        edit: Edit,
        group: Option<usize>,
        pieces: u64,
        bytes: u64,
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
                self.new_code(vec![HeldPiece::Literal(bytes.to_vec())])?
            }
            wire::OP_CODE_SPELL => self.spell(ints[0])?,
            wire::OP_CODE_COPY => {
                let r = self.range_arg(ints[0], ints[1])?;
                self.new_code(vec![HeldPiece::Copy(r)])?
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
            Effect::Code(pieces) => {
                fx.pieces += pieces.len() as u64;
                fx.piece_bytes += pieces.iter().map(HeldPiece::text_len).sum::<u64>();
                fx.codes.push(pieces);
                wire::handle(wire::TAG_CODE, fx.codes.len() as u64 - 1)
            }
            Effect::Group => {
                fx.record.groups.push(Vec::new());
                wire::handle(wire::TAG_GROUP, fx.record.groups.len() as u64 - 1)
            }
            Effect::Edit { edit, group, pieces, bytes } => {
                fx.pieces += pieces;
                fx.piece_bytes += bytes;
                fx.text += edit.notes.iter().map(|n| n.len() as u64).sum::<u64>();
                fx.record.edits.push(edit);
                let index = fx.record.edits.len() - 1;
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

    /// The pieces of code handle `h`; the null handle is the empty code.
    fn code_arg(&self, h: u64) -> Step<&[HeldPiece]> {
        if h == 0 {
            return Ok(&[]);
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
        Place { file: self.subject.file_ref(r.file), lo: r.lo, hi: r.hi }
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

    /// `Subject::tier` of `r`: 0 or 1, `None` when the subject cannot tell;
    /// any other answer is a host failure.
    fn tier(&self, r: Range) -> Step<Option<u8>> {
        match self.subject.tier(r) {
            t @ (None | Some(0 | 1)) => Ok(t),
            Some(t) => Err(Stop::Internal(format!("Subject::tier gave {t}; a tier is 0 or 1"))),
        }
    }

    // -----------------------------------------------------------------------
    // Code values (§7.1).

    /// A new code value of these (normalised, non-empty) pieces, if the
    /// limits allow.
    fn new_code(&self, pieces: Vec<HeldPiece>) -> Step<Effect> {
        if self.effects.codes.len() as u64 >= self.limits.code_values {
            return limit("code values", self.limits.code_values);
        }
        if pieces.len() as u64 > self.limits.pieces_per_code {
            return limit("pieces in one code value", self.limits.pieces_per_code);
        }
        let bytes = pieces.iter().map(HeldPiece::text_len).sum();
        self.pieces_fit(pieces.len() as u64, bytes)?;
        Ok(Effect::Code(pieces))
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
        let len: usize = names.iter().map(|n| n.len() + 2).sum();
        if len as u64 > self.limits.literal_bytes {
            return limit("bytes of text in pieces", self.limits.literal_bytes);
        }
        let mut qualified = Vec::with_capacity(len);
        let mut identifier_at = 0;
        for n in &names {
            qualified.extend_from_slice(b"::");
            identifier_at = qualified.len();
            qualified.extend_from_slice(n);
        }
        self.new_code(vec![HeldPiece::Spelled { entity: e, key, qualified, identifier_at }])
    }

    /// §7.1 rule 5 at L0: the pieces of the codes in order, adjacent
    /// literals joined. `None`: the result is the empty code.
    fn cat(&self, handles: &[u64]) -> Step<Option<Effect>> {
        let mut codes = Vec::with_capacity(handles.len().min(1 << 16));
        for &h in handles {
            codes.push(self.code_arg(h)?);
        }
        let max_pieces = self.limits.pieces_per_code;
        let max_bytes = self.limits.literal_bytes.saturating_sub(self.effects.piece_bytes);
        let mut out: Vec<HeldPiece> = Vec::new();
        let mut bytes = 0u64;
        for p in codes.into_iter().flatten() {
            bytes = bytes.saturating_add(p.text_len());
            if bytes > max_bytes {
                return limit("bytes of text in pieces", self.limits.literal_bytes);
            }
            if let (Some(HeldPiece::Literal(prev)), HeldPiece::Literal(b)) = (out.last_mut(), p) {
                prev.extend_from_slice(b);
                continue;
            }
            if out.len() as u64 >= max_pieces {
                return limit("pieces in one code value", max_pieces);
            }
            out.push(p.clone());
        }
        if out.is_empty() { Ok(None) } else { self.new_code(out).map(Some) }
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
        let Some(tier) = self.tier(r)? else {
            return refuse(
                Errc::Unsupported,
                "a range that does not lie in a name range or an extent of the unit, so its \
                 tier is not known (§7.3 rule 2)",
            );
        };
        if self.effects.record.edits.len() as u64 >= self.limits.edits {
            return limit("edits requested", self.limits.edits);
        }
        // §7.2: the sibling rule, or every spelled piece fully qualified.
        let sibling = match code {
            [HeldPiece::Spelled { entity, .. }] => self.sibling(r, *entity)?,
            _ => None,
        };
        let (range, tier, pieces) = match (sibling, code) {
            (Some(name), [HeldPiece::Spelled { key, qualified, identifier_at, .. }]) => {
                let Some(t) = self.tier(name)? else {
                    return Err(Stop::Internal(
                        "Subject::tier gave None for a name range it gave out".to_owned(),
                    ));
                };
                let text = qualified.get(*identifier_at..).unwrap_or_default().to_vec();
                (name, t, vec![Piece::Spelled { key: *key, text }])
            }
            _ => (r, tier, code.iter().map(|p| self.piece(p)).collect()),
        };
        let bytes = pieces
            .iter()
            .map(|p| match p {
                Piece::Literal(b) | Piece::Spelled { text: b, .. } => b.len() as u64,
                Piece::Copy(_) => 0,
            })
            .sum();
        self.pieces_fit(pieces.len() as u64, bytes)?;
        self.text_fits(note.len() as u64)?;
        let notes = if note.is_empty() { Vec::new() } else { vec![note.to_vec()] };
        let edit = Edit { place: self.place_of(range), pieces, tier, notes };
        Ok(Effect::Edit { pieces: edit.pieces.len() as u64, bytes, edit, group })
    }

    /// A held piece as recorded away from the sibling rule: a spelled
    /// piece takes its fully qualified text (§7.2 rule 2).
    fn piece(&self, p: &HeldPiece) -> Piece {
        match p {
            HeldPiece::Literal(b) => Piece::Literal(b.clone()),
            HeldPiece::Copy(r) => Piece::Copy(self.place_of(*r)),
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
        let s = self.subject;
        let refs = s.references_at(r);
        if refs.is_empty() {
            return Ok(None);
        }
        let parent = s.parent(e);
        if let Some(p) = parent {
            self.subject_value("parent", Value::Entity(p))?;
        }
        // Condition 1. Always true of an entity `spell` accepted (§7.1 rule
        // 4), and checked so that the rule reads as written.
        let mut holds = s.identifier(Value::Entity(e)).is_some_and(|id| !id.is_empty());
        let mut name: Option<Range> = None;
        let mut seen = BTreeSet::new();
        for &x in &refs {
            let v = self.subject_value("references_at", Value::Reference(x))?;
            if !seen.insert(x) {
                return Err(Stop::Internal(
                    "Subject::references_at gave a reference twice".to_owned(),
                ));
            }
            match s.extent(v) {
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
            holds &= s.parent(origin) == parent;
            // Condition 3: found directly.
            holds &= s.found_directly(x);
            // Condition 4, and one name range for all (§7.2 rule 1).
            match s.name_range(v) {
                Ok(Some(n)) => {
                    let n = self.subject_range("name_range", n)?;
                    if n.lo >= n.hi || !within(n, r) {
                        return Err(Stop::Internal(
                            "Subject::name_range gave a range that is empty or not inside the \
                             reference's extent"
                                .to_owned(),
                        ));
                    }
                    holds &= name.is_none_or(|m| m == n);
                    name = Some(n);
                }
                Ok(None) => holds = false,
                Err(errc) => {
                    return Err(self.subject_status("name_range", v, errc, ALLOWED_NAME_RANGE));
                }
            }
        }
        Ok(if holds { name } else { None })
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
                    Some((r, own)) => (Some(self.place_of(self.subject_range("place", r)?)), !own),
                    None => (None, false),
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
