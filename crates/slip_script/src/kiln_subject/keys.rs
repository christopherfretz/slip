//! Keys for declarations and spelled entities (`spec/api.md` §7.5 rules 1
//! to 3, §7.1 rule 4; `docs/wp/WP-095.md`, `spec/landed/api/WP-095.md`).
//!
//! **Nothing here computes a key.** The subject finds the declaration that
//! holds a reference or an expression (its *holder*) and asks
//! `slip_digest::query::Keys` (WP-087) for the unit that holds that
//! declaration, the unit's key and the units derived from it; for a
//! rename, the rule; for a spelled entity, its key id.
//!
//! **Where `Keys` lives.** `Keys` borrows a `slip_digest::Input`, whose
//! path function is not `Sync`, and needs a thread of
//! `slip_digest::STACK_SIZE`; the subject is `Sync` and is asked from the
//! transform's thread. So `Keys` runs on a thread of its own, the *key
//! server*, which holds the `Input` and the `Keys` on its stack and
//! answers one question at a time over a channel. The server lives for
//! the call of [`KilnSubject::with_keys`]: `Keys` is built on its first
//! question and kept until `with_keys` returns, so its memos (the keys,
//! the units, the one walk `Keys::derived` runs) are paid once per run.
//! A question asked outside `with_keys` starts a server for that one
//! question: the same answer, with nothing kept.
//!
//! **The holder of a reference** (one written name; possibly several
//! entries of Kiln's table, `references.rs`) is the holder every entry
//! gives, and an entry's holder is, from two sources that must agree:
//!
//! - the declaration the walk of the calls index (WP-086) entered to reach
//!   the expression whose name the entry is (`calls.rs`, `ref_holder`):
//!   names of expressions in bodies, initialisers, default arguments,
//!   template heads, types as written;
//! - the declaration Kiln's table attributes the entry to (its
//!   `enclosing`, Kiln `spec/ast.md` §18.3: the innermost declaration
//!   whose extent holds the name's first token, as claimed). A namespace,
//!   the translation unit, a linkage specification or an enumeration is
//!   no answer: Kiln gives the lexical context there for the names of the
//!   declarations that claim none (an alias, an enumerator, a
//!   `static_assert`, a template head), so the declaration that holds the
//!   name is not known. A class is the answer only for a name in its
//!   head (before its body: a base specifier, the arguments of a
//!   specialisation), for the same reason. A name in the decl-specifiers
//!   shared by a declarator group of two declarators or more (`S a, b;`)
//!   is held by each: no holder.
//!
//! Every holder found is turned into its unit by `Keys::unit_of`; the
//! units must be one, or the answer is `unsupported`.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{MutexGuard, PoisonError};

use slip_digest::query::{Keys, NoKey};
use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::source::{FileId, Loc};
use slip_model::Digest;
use slip_model::keys::Rule;

use super::KilnSubject;
use crate::subject::{Expression, Reference};
use crate::wire::Errc;

/// A question to the key server.
enum Ask {
    /// The keys of the unit that holds every one of these declarations
    /// (§7.5 rule 1).
    Unit(Vec<DeclId>),
    /// `Keys::key_id` of an entity.
    Key(DeclId),
    /// `Keys::rename_rule`.
    Rename(DeclId, Vec<u8>),
    /// `Keys::entity` of each declaration (test 5 of the package).
    Entities(Vec<DeclId>),
    /// The unit `Keys::unit_of` gives each declaration (for tests).
    Units(Vec<DeclId>),
}

/// The key server's answer.
enum Answer {
    Unit(Result<Vec<Digest>, Why>),
    Key(Option<Digest>),
    Rule(Result<Rule, NoKey>),
    Decls(Vec<Option<DeclId>>),
}

/// Why `unit_keys` is `unsupported`, for the tests and the handback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The value is not a reference or an expression (the host asks for
    /// no other sort).
    NotHeld,
    /// No holder is known for the reference or the expression, or its
    /// entries give two.
    NoHolder,
    /// `Keys::unit_of` gives no unit for a holder.
    NoUnit,
    /// The holders are in two units.
    TwoUnits,
    /// The unit's key: `Keys::key_id` refused it.
    Key(NoKey),
    /// The key server could not be asked (its thread did not start, or
    /// stopped).
    NoServer,
}

/// The subject's end of a running key server.
pub(super) struct Link {
    ask: Sender<Ask>,
    answer: Receiver<Answer>,
}

/// What Kiln's table says of one entry, read on first use.
#[derive(Default)]
pub(super) struct Tables {
    /// Per entry of `KilnSubject::refs`: the table's `enclosing`.
    enclosing: Vec<Option<DeclId>>,
    /// Per `full` span of a declaration-extent row with decl-specifiers
    /// (raw `lo`, `hi`): how many declarations as written share it.
    groups: HashMap<(u32, u32), u32>,
}

/// What the table's attribution of one entry gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attributed {
    /// The holder.
    Holder(DeclId),
    /// The table gives a context whose names may be another
    /// declaration's: no answer from this source.
    Unknown,
    /// The entry is held by two declarations, or by a declaration Kiln
    /// made: no holder at all.
    Refused,
}

/// The stack of the key server's thread: the walk's, and a margin.
const SERVER_STACK: usize = slip_digest::STACK_SIZE + (1 << 20);

impl<'u> KilnSubject<'u> {
    /// Runs `f` with this subject's key server running: every key
    /// question `f` asks (`unit_keys`, `rename_rule`, `entity_key`) is
    /// answered by one `slip_digest::query::Keys`, built on the first and
    /// kept until `f` returns (module documentation). Nested calls share
    /// the server. A run of a transform over this subject is made inside
    /// it (`docs/wp/WP-095.md` item 4).
    pub fn with_keys<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        let mut link = self.link();
        if link.is_some() {
            drop(link);
            return f(self);
        }
        std::thread::scope(|s| {
            let (ask, asked) = channel::<Ask>();
            let (answer, answered) = channel::<Answer>();
            let started = std::thread::Builder::new()
                .name("slip-keys".to_owned())
                .stack_size(SERVER_STACK)
                .spawn_scoped(s, move || self.serve(&asked, &answer));
            if started.is_ok() {
                *link = Some(Link { ask, answer: answered });
            }
            drop(link);
            // Ends the server when `f` returns or unwinds: the scope joins
            // the server's thread, which ends when its channel closes.
            let _end = EndServer(self);
            f(self)
        })
    }

    fn link(&self) -> MutexGuard<'_, Option<Link>> {
        self.keys.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Asks the running server, or one started for this question.
    fn ask(&self, q: Ask) -> Option<Answer> {
        {
            let link = self.link();
            if let Some(l) = link.as_ref() {
                // The lock is held for the question and its answer, so
                // answers are never crossed.
                l.ask.send(q).ok()?;
                return l.answer.recv().ok();
            }
        }
        self.with_keys(|s| {
            let link = s.link();
            let l = link.as_ref()?;
            l.ask.send(q).ok()?;
            l.answer.recv().ok()
        })
    }

    /// The key server's loop (module documentation).
    fn serve(&self, asked: &Receiver<Ask>, answer: &Sender<Answer>) {
        let path = |f: FileId| self.path(f).map(<[u8]>::to_vec).unwrap_or_default();
        let input = slip_digest::Input { ast: self.ast, sm: self.sm, subject_path: &path };
        let mut keys: Option<Keys<'_>> = None;
        while let Ok(q) = asked.recv() {
            let k = keys.get_or_insert_with(|| Keys::new(&input));
            let a = match q {
                Ask::Unit(holders) => Answer::Unit(unit_keys_of(k, &holders)),
                Ask::Key(e) => Answer::Key(k.key_id(e).ok()),
                Ask::Rename(e, n) => Answer::Rule(k.rename_rule(e, &n)),
                Ask::Entities(ds) => Answer::Decls(ds.iter().map(|&d| Some(k.entity(d))).collect()),
                Ask::Units(ds) => Answer::Decls(ds.iter().map(|&d| k.unit_of(d)).collect()),
            };
            if answer.send(a).is_err() {
                break;
            }
        }
    }

    // ---- the three questions ----------------------------------------------

    /// §7.5 rule 1: the keys of the unit that holds reference `r`, or why
    /// there are none.
    pub fn reference_unit_keys(&self, r: Reference) -> Result<Vec<Digest>, Why> {
        let holders = self.reference_holders(r).ok_or(Why::NoHolder)?;
        self.unit_keys_of_holders(holders)
    }

    /// §7.5 rule 1: the keys of the unit that holds expression `x`, or why
    /// there are none.
    pub fn expression_unit_keys(&self, x: Expression) -> Result<Vec<Digest>, Why> {
        let h = self.holder(x).ok_or(Why::NoHolder)?;
        self.unit_keys_of_holders(vec![h])
    }

    fn unit_keys_of_holders(&self, holders: Vec<DeclId>) -> Result<Vec<Digest>, Why> {
        match self.ask(Ask::Unit(holders)) {
            Some(Answer::Unit(r)) => r,
            _ => Err(Why::NoServer),
        }
    }

    pub(super) fn entity_key_of(&self, e: DeclId) -> Option<Digest> {
        match self.ask(Ask::Key(e))? {
            Answer::Key(k) => k,
            _ => None,
        }
    }

    /// §7.5 rule 3: `Keys::rename_rule`, every refusal `unsupported` (the
    /// host has judged the identifier before asking; a `BadName` here is
    /// the two readings of an identifier disagreeing, and refused).
    pub(super) fn rename_rule_of(&self, e: DeclId, name: &[u8]) -> Result<Rule, Errc> {
        match self.ask(Ask::Rename(e, name.to_vec())) {
            Some(Answer::Rule(Ok(r))) => Ok(r),
            _ => Err(Errc::Unsupported),
        }
    }

    /// For test 5 of the package: each declaration of the arena whose
    /// entity the subject and `Keys::entity` give differently, as
    /// `(declaration, subject's, Keys')` indexes. `None` when the key
    /// server could not be asked.
    #[doc(hidden)]
    pub fn entity_disagreements(&self) -> Option<Vec<(u32, u32, u32)>> {
        let ds: Vec<DeclId> = (1..=self.ast.decls.len() as u32).map(DeclId::new).collect();
        let Answer::Decls(theirs) = self.ask(Ask::Entities(ds.clone()))? else { return None };
        Some(
            ds.iter()
                .zip(theirs)
                .filter_map(|(&d, t)| {
                    let t = t?;
                    let ours = self.entity(d);
                    (ours != t).then(|| (d.index(), ours.index(), t.index()))
                })
                .collect(),
        )
    }

    /// For tests: the holders of reference `r` (module documentation),
    /// and the unit `Keys::unit_of` gives each, as declaration indexes.
    #[doc(hidden)]
    pub fn reference_holder_units(&self, r: Reference) -> Option<Vec<(u32, Option<u32>)>> {
        let hs = self.reference_holders(r)?;
        let Answer::Decls(us) = self.ask(Ask::Units(hs.clone()))? else { return None };
        Some(hs.iter().zip(us).map(|(h, u)| (h.index(), u.map(DeclId::index))).collect())
    }

    // ---- the holder of a reference ------------------------------------------

    /// The holders of every entry of `r`, without repeats; `None` when an
    /// entry has none or is refused (module documentation).
    fn reference_holders(&self, r: Reference) -> Option<Vec<DeclId>> {
        let entries = self.reference_entries(r);
        if entries.is_empty() {
            return None;
        }
        let mut out: Vec<DeclId> = Vec::new();
        for j in entries {
            let mut found = false;
            match self.entry_holder(j) {
                Some(Some(h)) => {
                    out.push(h);
                    found = true;
                }
                Some(None) => return None,
                None => {}
            }
            match self.attributed(j) {
                Attributed::Holder(h) => {
                    out.push(h);
                    found = true;
                }
                Attributed::Refused => return None,
                Attributed::Unknown => {}
            }
            if !found {
                return None;
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }

    fn tables(&self) -> &Tables {
        self.key_tables.get_or_init(|| self.build_tables())
    }

    fn build_tables(&self) -> Tables {
        let ast = self.ast;
        // Kiln's entries joined as `slip_kiln::written` joins them: by
        // role, range and referent, the first kept.
        let mut by: HashMap<(u8, u32, u32, u32), Option<DeclId>> = HashMap::new();
        for (_, e) in ast.written_refs.iter() {
            let Some(x) = e.entity else { continue };
            by.entry((e.role as u8, e.range.lo.to_raw(), e.range.hi.to_raw(), x.index()))
                .or_insert(e.enclosing);
        }
        let enclosing = self
            .refs
            .iter()
            .map(|r| {
                let k = (r.role as u8, r.name.lo.to_raw(), r.name.hi.to_raw(), r.referent.index());
                by.get(&k).copied().flatten()
            })
            .collect();
        let mut seen: HashMap<(u32, u32), Vec<DeclId>> = HashMap::new();
        for (d, row) in ast.decl_extents.iter() {
            if row.specs_end.is_valid() {
                let v = seen.entry((row.full.lo.to_raw(), row.full.hi.to_raw())).or_default();
                let d = self.as_written(d);
                if !v.contains(&d) {
                    v.push(d);
                }
            }
        }
        let groups = seen.into_iter().map(|(k, v)| (k, v.len() as u32)).collect();
        Tables { enclosing, groups }
    }

    /// What the table's `enclosing` gives entry `j` (module
    /// documentation).
    fn attributed(&self, j: u32) -> Attributed {
        let ast = self.ast;
        let Some(&Some(e)) = self.tables().enclosing.get(j as usize) else {
            return Attributed::Unknown;
        };
        let Some(r) = self.refs.get(j as usize) else { return Attributed::Unknown };
        let at = r.name.lo;
        if self.instantiated_or_implicit(e) {
            return Attributed::Refused;
        }
        let kind = self.kind(e);
        if matches!(
            kind,
            DeclKind::TranslationUnit
                | DeclKind::Namespace
                | DeclKind::LinkageSpec
                | DeclKind::Export
                | DeclKind::Enum
                | DeclKind::RequiresExprBody
        ) {
            return Attributed::Unknown;
        }
        let row = ast.decl_extents.get(e).or_else(|| ast.decl_extents.get(self.as_written(e)));
        if ast.decls.record(e).is_some() {
            // A class: only a name of its head is its own.
            return match row {
                Some(row)
                    if row.body.lo.is_valid() && self.before(row.full.lo, at, row.body.lo) =>
                {
                    Attributed::Holder(e)
                }
                _ => Attributed::Unknown,
            };
        }
        if let Some(row) = row
            && row.specs_end.is_valid()
            && self.before(row.full.lo, at, row.specs_end)
        {
            let shared = self
                .tables()
                .groups
                .get(&(row.full.lo.to_raw(), row.full.hi.to_raw()))
                .copied()
                .unwrap_or(0);
            if shared > 1 {
                return Attributed::Refused;
            }
        }
        Attributed::Holder(e)
    }

    /// `lo <= at < hi`, all three in one entry of the source manager (a
    /// file, or one macro expansion); false when that is not known.
    fn before(&self, lo: Loc, at: Loc, hi: Loc) -> bool {
        self.sm.same_entry_distance(lo, at).is_some()
            && self.sm.same_entry_distance(at, hi).is_some_and(|d| d > 0)
    }
}

/// Clears the subject's link when [`KilnSubject::with_keys`]'s `f`
/// returns or unwinds, which closes the server's channel.
struct EndServer<'a, 'u>(&'a KilnSubject<'u>);

impl Drop for EndServer<'_, '_> {
    fn drop(&mut self) {
        *self.0.link() = None;
    }
}

/// On the server: the unit that holds every one of `holders`, its key,
/// then the keys `Keys::derived` gives (sorted, the unit's own left out).
fn unit_keys_of(k: &mut Keys<'_>, holders: &[DeclId]) -> Result<Vec<Digest>, Why> {
    let mut unit: Option<DeclId> = None;
    for &h in holders {
        let u = k.unit_of(h).ok_or(Why::NoUnit)?;
        if unit.is_some_and(|x| x != u) {
            return Err(Why::TwoUnits);
        }
        unit = Some(u);
    }
    let u = unit.ok_or(Why::NoHolder)?;
    let own = k.key_id(u).map_err(Why::Key)?;
    let mut out = vec![own];
    out.extend(k.derived(u).into_iter().filter(|&d| d != own));
    Ok(out)
}
