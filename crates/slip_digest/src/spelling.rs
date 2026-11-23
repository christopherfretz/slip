//! Spelled entities in one observer (`spec/verify.md` §10.2, §10.6;
//! `docs/wp/WP-096.md`; what the spec leaves open is decided in
//! `spec/landed/verify/WP-096.md`).
//!
//! A spelled-entity piece says which entity a name is to denote and
//! leaves the text to Slip (`spec/api.md` §7.2); the native rename writes
//! one at every resolved reference it renames (`spec/edits.md` §9.3).
//! Where the piece's text landed in the post-image is an obligation
//! (§10.1). [`check_spellings`] answers, for one parse of the post-image,
//! whether the name there denotes the entity the obligation records.
//!
//! This is the trusted base. A wrong [`Spelling::Holds`] lets a broken
//! refactor through as verified; a wrong `Fails` or `NotSeen` is a false
//! alarm. So every doubt is `NotSeen` or `Fails`:
//!
//! - **Nothing is computed a second way.** The written references are
//!   `slip_kiln::written::written_refs`, the name range of one is
//!   `slip_kiln::ranges::file_range_of_span` (Kiln's
//!   `editable_file_range`), the origin of a referent is
//!   `slip_kiln::written::origin`, and the key of an origin is
//!   [`Keys::key_id`] under the run's rules ([`Keys::with_rules`]), so
//!   it is the key the walk of the same parse gives the entity. Whether a
//!   key collides (digest §3.4) is the walk's own marking
//!   (`units::mark_collisions`, `keys::mark_mapped_collisions`), run on
//!   the walk's own units.
//! - **A reference whose name has no range takes no part**: Kiln gives
//!   none, `ranges` refuses one (an empty span, a directive line, a
//!   `<scratch space>` buffer), the file's text is not its bytes (Kiln
//!   replaced invalid UTF-8, `spec/edits.md` §2.3 `file_recoded`), or the
//!   buffer has no subject path.
//! - **A key that is not one entity's is no key**: a key that collides
//!   in the unit is treated as an indeterminate key, and fails.

use std::collections::{HashMap, HashSet};

use slip_kiln::Unit;
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::source::{FileId, SourceManager};
use slip_kiln::ranges;
use slip_kiln::written::{self, Role, WrittenRef};
use slip_model::Digest;
use slip_model::keys::{KeyKind, RuleTable};

use crate::query::Keys;
use crate::{Cause, Detail, Input, Options};

/// One obligation of `spec/verify.md` §10.1 as the check takes it: the
/// file by its subject path (digest §11), the bytes `[lo, hi)` of its
/// post-image where the text of one spelled piece landed, and the key id
/// the piece carries (in the pre-image's key space).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obligation {
    /// The subject path of the file.
    pub file: Vec<u8>,
    pub lo: u32,
    pub hi: u32,
    pub key: Digest,
}

/// What one observer says about one obligation (`spec/verify.md` §10.2
/// rule 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spelling {
    /// At least one reference answers, and every answering reference's
    /// origin has the obligation's key.
    Holds,
    /// No reference answers.
    NotSeen,
    /// Some answering reference's origin has another key, or none.
    Fails {
        /// The key, under the rules, of the origin of the first answering
        /// reference in unit order that does not have the obligation's
        /// key; `None` when it has none (it is not a unit, is keyed by
        /// content, its key is indeterminate or collides).
        found: Option<Digest>,
        /// That entity as a person would write it (`spec/verify.md`
        /// §6.1): for the report only.
        found_name: String,
    },
}

/// Checks the obligations whose file this unit read, in one parse of the
/// post-image (`spec/verify.md` §10.2). The result is parallel to
/// `obligations`.
///
/// `unit` and `input` are of the same parse: `input` is the digest's
/// view of `unit` (its `ast` and `sm` are `unit`'s) with the subject
/// paths of the run. If they are not (a caller's error), every answer is
/// [`Spelling::NotSeen`]. An obligation whose `file` is a path the unit
/// did not read, or whose `lo` is not below `hi`, is `NotSeen`: no
/// reference can answer it.
///
/// **The unit must have been parsed with
/// [`ParseOptions::written`](slip_kiln::ParseOptions::written)**, as the
/// verify job of a transform's run always does (`docs/wp/WP-074.md`).
/// Without the tables (`slip_kiln::written::recorded` is false) no
/// reference is known and every obligation is `NotSeen`, which over the
/// run is `unseen`: safe, with the wrong message.
///
/// Under a table [`RuleTable::refused`] refuses, no entity has a key
/// (`Keys::with_rules`): an obligation some reference answers fails with
/// `found` absent, one none answers is `NotSeen`. Never `Holds`.
///
/// Runs on a thread of [`crate::STACK_SIZE`], as [`crate::digest`] does:
/// it computes keys, and runs the walk once over the unit (to know which
/// keys collide) when some obligation would hold.
pub fn check_spellings(
    unit: &Unit<'_>,
    input: &Input<'_>,
    rules: &RuleTable,
    obligations: &[Obligation],
) -> Vec<Spelling> {
    let not_seen = || vec![Spelling::NotSeen; obligations.len()];
    if obligations.is_empty() {
        return Vec::new();
    }
    if !std::ptr::eq(unit.ast(), input.ast) || !std::ptr::eq(unit.sm(), input.sm) {
        debug_assert!(false, "check_spellings: the unit and the input are of two parses");
        return not_seen();
    }
    if !written::recorded(unit) {
        return not_seen();
    }
    let placed = placed_refs(unit, input);
    let mut check = Check { unit, input, keys: Keys::with_rules(input, rules), rules, origins: HashMap::new(), walk: None };
    obligations.iter().map(|ob| check.one(&placed, ob)).collect()
}

/// A written reference that takes part (§10.2 rules 1 and 2), with its
/// name range.
struct Placed {
    /// The subject path of the range's file.
    path: Vec<u8>,
    lo: u32,
    hi: u32,
    r: WrittenRef,
}

/// Whether an entry of role `role` takes part (§10.2 rule 1: the roles of
/// `spec/api.md` §6.5 rule 3). Every role is listed, so a role Kiln adds
/// does not compile until it is placed here (§10.2: "a role Kiln adds
/// later takes part from the day `spec/api.md` announces it").
fn takes_part(role: Role) -> bool {
    match role {
        Role::Expr
        | Role::Member
        | Role::Nns
        | Role::Type
        | Role::Template
        | Role::Base
        | Role::MemInit => true,
        Role::UsingDecl
        | Role::UsingDirective
        | Role::NamespaceAlias
        | Role::Elaborated
        | Role::SpecTarget
        | Role::Concept
        | Role::Designator => false,
    }
}

/// The references of the unit that take part, with their name ranges, in
/// unit order (`slip_kiln::ranges::unit_order` of their names' starts;
/// the table's order among equal ones).
fn placed_refs(unit: &Unit<'_>, input: &Input<'_>) -> Vec<Placed> {
    let sm = unit.sm();
    let mut refs: Vec<WrittenRef> =
        written::written_refs(unit).into_iter().filter(|r| takes_part(r.role)).collect();
    refs.sort_by(|a, b| ranges::unit_order(sm, a.name.lo, b.name.lo));
    let mut paths: HashMap<FileId, Vec<u8>> = HashMap::new();
    let mut out = Vec::new();
    for r in refs {
        let Some(fr) = ranges::file_range_of_span(sm, r.name) else { continue };
        if !text_is_bytes(sm, fr.file) {
            continue;
        }
        let path = paths.entry(fr.file).or_insert_with(|| (input.subject_path)(fr.file));
        if path.is_empty() {
            continue;
        }
        out.push(Placed { path: path.clone(), lo: fr.lo, hi: fr.hi, r });
    }
    out
}

/// Whether Kiln's text of `file` is its bytes, so that offsets in one are
/// offsets in the other (`spec/edits.md` §2.3, `file_recoded`).
fn text_is_bytes(sm: &SourceManager, file: FileId) -> bool {
    sm.load_report(file).is_clean()
}

/// The keys that collide in this unit (digest §3.4), as the walk marks
/// them.
struct Collisions {
    /// Keys of units the walk marks `Collision`.
    keys: HashSet<Digest>,
    /// Keys of units the walk writes (but the order unit).
    with_unit: HashSet<Digest>,
    /// The walk marked the order unit: some key that no unit has collides
    /// (`keys::mark_mapped_collisions`), which one is not said.
    unitless: bool,
    /// The names of the walk's units, by key (`spec/verify.md` §6.1).
    names: HashMap<Digest, String>,
}

impl Collisions {
    /// Whether `k` may be the key of two entities.
    fn collides(&self, k: &Digest) -> bool {
        self.keys.contains(k) || (self.unitless && !self.with_unit.contains(k))
    }
}

struct Check<'a, 'u> {
    unit: &'a Unit<'u>,
    input: &'a Input<'a>,
    keys: Keys<'a>,
    rules: &'a RuleTable,
    /// The key of each origin asked for, memoised: `None` when it has
    /// none or its key is not one entity's.
    origins: HashMap<DeclId, Option<Digest>>,
    /// The walk's collisions and names, made on first need.
    walk: Option<Collisions>,
}

impl Check<'_, '_> {
    fn one(&mut self, placed: &[Placed], ob: &Obligation) -> Spelling {
        if ob.lo >= ob.hi || ob.file.is_empty() {
            return Spelling::NotSeen;
        }
        // §10.2 rule 3: in the file, ending at `hi`, starting at or after
        // `lo`; in unit order.
        let answering: Vec<&Placed> = placed
            .iter()
            .filter(|p| p.path == ob.file && p.hi == ob.hi && p.lo >= ob.lo)
            .collect();
        if answering.is_empty() {
            return Spelling::NotSeen;
        }
        // §10.2 rules 4 and 5: every answering reference's origin must
        // have the key; the first that does not is what was found.
        let mut first_other: Option<(Option<Digest>, DeclId)> = None;
        for p in &answering {
            let origin = written::origin(self.unit, p.r.referent);
            let key = self.key(origin);
            if key != Some(ob.key) && first_other.is_none() {
                first_other = Some((key, origin));
            }
        }
        match first_other {
            None => Spelling::Holds,
            Some((found, origin)) => {
                let found_name = self.name(found.as_ref(), origin);
                Spelling::Fails { found, found_name }
            }
        }
    }

    /// The key of `origin` under the rules, or `None` when it has none or
    /// it collides.
    fn key(&mut self, origin: DeclId) -> Option<Digest> {
        if let Some(&k) = self.origins.get(&origin) {
            return k;
        }
        let key = match self.keys.key_id(origin) {
            Ok(k) if !self.walk().collides(&k) => Some(k),
            Ok(_) | Err(_) => None,
        };
        self.origins.insert(origin, key);
        key
    }

    /// The walk's collisions and names, made on first use.
    fn walk(&mut self) -> &Collisions {
        let (input, rules) = (self.input, self.rules);
        self.walk.get_or_insert_with(|| collisions(input, rules))
    }

    /// The name of the entity found (`spec/verify.md` §6.1): the walk's
    /// name of its key's unit, else the host's name of the entity
    /// (`display_name`), which is §6.1's qualified form without what
    /// needs the walk.
    fn name(&mut self, found: Option<&Digest>, origin: DeclId) -> String {
        if let Some(k) = found
            && let Some(n) = self.walk().names.get(k)
        {
            return n.clone();
        }
        let e = self.keys.entity(origin);
        let mut n = crate::display_name(self.input.ast, e);
        if n.len() > NAME_LIMIT {
            let mut cut = NAME_LIMIT;
            while !n.is_char_boundary(cut) {
                cut -= 1;
            }
            n.truncate(cut);
        }
        n
    }
}

/// A name is cut to this many bytes (`spec/verify.md` §6.1).
const NAME_LIMIT: usize = 4096;

/// The walk of `input` under `rules`, for which keys collide and the
/// units' names: the walk's own units, with their causes put aside and
/// marked again by the walk's two functions, so that a collision is seen
/// whatever other cause a unit has (a unit keeps the first cause met).
fn collisions(input: &Input<'_>, rules: &RuleTable) -> Collisions {
    let options = Options { rules, detail: Detail::None, names: true };
    crate::walk::run_with(input, &options, |w, written, output| {
        let mut again: Vec<crate::units::Written> = written.to_vec();
        for u in &mut again {
            u.cause = None;
        }
        crate::units::mark_collisions(&mut again);
        crate::keys::mark_mapped_collisions(w, input, &mut again);
        let mut keys = HashSet::new();
        let mut with_unit = HashSet::new();
        let mut unitless = false;
        for u in &again {
            if u.kind == KeyKind::Order {
                unitless |= u.cause == Some(Cause::Collision);
                continue;
            }
            with_unit.insert(u.key);
            if u.cause == Some(Cause::Collision) {
                keys.insert(u.key);
            }
        }
        let names = output.names.into_iter().collect();
        Collisions { keys, with_unit, unitless, names }
    })
}
