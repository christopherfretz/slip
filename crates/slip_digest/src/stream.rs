//! `Stream`: the only way bytes reach a hash (`spec/digest.md` §4).
//!
//! A stream is the bytes of one unit, type, discriminator or key, in the
//! primitives of `spec/model.md` §2, together with the first reason it
//! cannot be trusted (`spec/digest.md` §10). Nested streams (a type inside
//! a unit) hand their cause to the stream that writes their digest
//! ([`Stream::absorb`]).
//!
//! # The recorder (WP-054)
//!
//! A stream can be *traced* (`spec/verify.md` §6.1 "Traces"): it then
//! keeps, beside its bytes, the offset at which the walk began each
//! statement and expression ([`Stream::mark`], the hooks of
//! `expr::write_node` and `stmt::node`) and the offset of each `digest`
//! field. Only `crate::trace` makes one, after the walk, for the second
//! writing of a unit: [`Stream::traced`] for a unit it writes itself, and
//! [`Armed`] for the parts `demand::resolved_units` writes, where each
//! stream made while no traced stream is open records, and each such
//! stream hashed as a unit is kept. Every other stream, which is every
//! stream of the ordinary walk and every sub-stream of a traced one, has
//! no recorder: a mark is a test of an `Option` that is `None`, and no
//! byte, no hash and no allocation depends on it.

use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::wire::Encoder;

#[cfg(test)]
use crate::Area;
use crate::Cause;
use crate::tables::Row;
use crate::walk::NodeRef;

/// `H("slip.unit.v1", stream)`: a unit's digest (`spec/model.md` §3,
/// `spec/digest.md` §4).
pub(crate) const UNIT_TAG: &str = "slip.unit.v1";
/// `H("slip.type.v1", stream)`: a type digest (`spec/digest.md` §4.2).
pub(crate) const TYPE_TAG: &str = "slip.type.v1";

/// What a pending area writes in place of its rows (WP-024): a `uv` 0,
/// which no tag is (`spec/digest-kinds.md` §1.4), so the placeholder is
/// fixed and the stream stays well formed. No area is pending since
/// WP-045 removed `Area::Demand`, so only the tests below write it; a
/// package that adds a pending area removes the `cfg`.
#[cfg(test)]
const PENDING_PLACEHOLDER: u64 = 0;

/// The bytes of one stream and the first cause that makes it
/// indeterminate, with what back-references need of it
/// (`spec/digest.md` §4.3): an identity, and the count of the nodes it
/// wrote first.
#[derive(Debug)]
pub(crate) struct Stream {
    enc: Encoder,
    cause: Option<Cause>,
    /// Unique among the streams of this thread: a back-reference names a
    /// node of its own stream only (`walk::Walk::back_ref`). Never
    /// written.
    id: u64,
    /// The count the next node written first takes, from 0 at the head
    /// of the stream.
    nodes: u64,
    /// What a traced stream records (WP-054); `None` for every other.
    /// Never written.
    rec: Option<Box<Recorder>>,
}

/// What a traced stream records ("The recorder" above).
#[derive(Debug, Default)]
pub(crate) struct Recorder {
    /// The stream's length where the walk began each node, and the node.
    marks: Vec<(u64, NodeRef)>,
    /// The offset of each `digest` field.
    digests: Vec<u64>,
    /// Made by an armed stream module ([`Armed`]): when it ends, the
    /// module may make the next one.
    armed: bool,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        #[cfg(test)]
        LIVE.with(|l| l.set(l.get() - 1));
        if self.armed {
            ARMING.with(|a| {
                if a.get() == BUSY {
                    a.set(ARMED);
                }
            });
        }
    }
}

/// The bytes of a traced stream and what it recorded.
#[derive(Clone, Debug)]
pub(crate) struct Recording {
    pub bytes: Vec<u8>,
    pub marks: Vec<(u64, NodeRef)>,
    pub digests: Vec<u64>,
}

/// The states of [`ARMING`].
const OFF: u8 = 0;
const ARMED: u8 = 1;
const BUSY: u8 = 2;

thread_local! {
    /// The identity the next stream of this thread takes. A walk runs on
    /// one thread and its memo of back-references lives no longer than
    /// it does, so identities unique per thread are enough.
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// [`OFF`] but while [`Armed`] lives: then [`ARMED`] when the next
    /// stream made records, [`BUSY`] while the one that does is open.
    static ARMING: std::cell::Cell<u8> = const { std::cell::Cell::new(OFF) };
    /// The traced streams an armed module hashed as units, in order.
    static KEPT: std::cell::RefCell<Vec<Recording>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    /// How many recorders this thread has made (`trace::tests`).
    pub(crate) static RECORDERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// How many recorders are open, and the most ever open at once.
    static LIVE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(crate) static MAX_LIVE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl Default for Stream {
    fn default() -> Stream {
        let id = NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        let rec = if ARMING.with(std::cell::Cell::get) == ARMED { armed_recorder() } else { None };
        Stream { enc: Encoder::default(), cause: None, id, nodes: 0, rec }
    }
}

/// The recorder of the stream an armed module makes next ([`Armed`]).
#[cold]
#[inline(never)]
fn armed_recorder() -> Option<Box<Recorder>> {
    ARMING.with(|a| a.set(BUSY));
    let mut r = recorder();
    r.armed = true;
    Some(r)
}

/// A new recorder.
fn recorder() -> Box<Recorder> {
    #[cfg(test)]
    {
        RECORDERS.with(|c| c.set(c.get() + 1));
        LIVE.with(|l| l.set(l.get() + 1));
        let live = LIVE.with(std::cell::Cell::get);
        MAX_LIVE.with(|m| m.set(m.get().max(live)));
    }
    Box::default()
}

/// While it lives, the stream module of this thread is armed: each stream
/// made while no traced stream is open records, and each such stream
/// hashed with [`UNIT_TAG`] is kept ([`Armed::take`]). For the second
/// writing of the units `demand::resolved_units` writes (`trace.rs`).
pub(crate) struct Armed(());

impl Armed {
    pub(crate) fn new() -> Armed {
        KEPT.with(|k| k.borrow_mut().clear());
        ARMING.with(|a| a.set(ARMED));
        Armed(())
    }

    /// The streams kept so far, in the order they were hashed.
    pub(crate) fn take(&self) -> Vec<Recording> {
        KEPT.with(|k| std::mem::take(&mut *k.borrow_mut()))
    }
}

impl Drop for Armed {
    fn drop(&mut self) {
        ARMING.with(|a| a.set(OFF));
        KEPT.with(|k| k.borrow_mut().clear());
    }
}

/// The result of hashing a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hashed {
    pub digest: Digest,
    pub cause: Option<Cause>,
    /// The bytes hashed (`digest.bytes`, `spec/digest.md` §12).
    pub bytes: u64,
}

impl Stream {
    pub(crate) fn new() -> Stream {
        Stream::default()
    }

    /// A stream that records (WP-054, "The recorder" above): for the
    /// second writing of a unit only.
    pub(crate) fn traced() -> Stream {
        let mut s = Stream::default();
        if s.rec.is_none() {
            s.rec = Some(recorder());
        }
        s
    }

    /// The hook of a node (WP-054): where the walk begins a statement or
    /// an expression. Nothing unless the stream is traced.
    #[inline]
    pub(crate) fn mark(&mut self, node: NodeRef) {
        if self.rec.is_some() {
            self.record_mark(node);
        }
    }

    #[cold]
    #[inline(never)]
    fn record_mark(&mut self, node: NodeRef) {
        let at = self.enc.as_bytes().len() as u64;
        if let Some(r) = &mut self.rec {
            r.marks.push((at, node));
        }
    }

    /// The bytes of a traced stream and what it recorded, not hashed.
    pub(crate) fn into_recording(self) -> Recording {
        let Stream { enc, rec, .. } = self;
        let (marks, digests) = rec.map_or_else(Default::default, |mut r| {
            (std::mem::take(&mut r.marks), std::mem::take(&mut r.digests))
        });
        Recording { bytes: enc.into_bytes(), marks, digests }
    }

    /// `uv`.
    pub(crate) fn uv(&mut self, v: u64) -> &mut Self {
        self.enc.uv(v);
        self
    }

    /// `sv`.
    #[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
    pub(crate) fn sv(&mut self, v: i64) -> &mut Self {
        self.enc.sv(v);
        self
    }

    /// `bool`.
    pub(crate) fn bool(&mut self, v: bool) -> &mut Self {
        self.enc.bool(v);
        self
    }

    /// `bytes`.
    pub(crate) fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.enc.bytes(v);
        self
    }

    /// `digest`.
    pub(crate) fn digest(&mut self, v: &Digest) -> &mut Self {
        if let Some(r) = &mut self.rec {
            r.digests.push(self.enc.as_bytes().len() as u64);
        }
        self.enc.digest(v);
        self
    }

    /// A `Name` (`spec/digest-kinds.md` §3.2), as `slip_model` encodes it.
    #[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
    pub(crate) fn name(&mut self, n: &slip_model::keys::Name) -> &mut Self {
        n.encode(&mut self.enc);
        self
    }

    /// Bytes another stream encoded (the elements of `*sorted`).
    pub(crate) fn raw(&mut self, v: &[u8]) -> &mut Self {
        for &b in v {
            self.enc.u8(b);
        }
        self
    }

    /// The count of a list (`*`, `spec/digest-kinds.md` §1.3).
    pub(crate) fn count(&mut self, n: usize) -> &mut Self {
        self.enc.count(n);
        self
    }

    /// The presence byte of an optional (`?`).
    pub(crate) fn opt(&mut self, present: bool) -> &mut Self {
        self.enc.bool(present);
        self
    }

    /// `tag N` of a variant row whose decision the caller has checked.
    pub(crate) fn tag(&mut self, row: Row) -> &mut Self {
        self.enc.uv(u64::from(row.tag));
        self
    }

    /// A record's flag word (`spec/digest-kinds.md` §2.1), as `uv`.
    pub(crate) fn flags(&mut self, word: FlagWord) -> &mut Self {
        self.enc.uv(word.bits);
        if word.n > 64 {
            self.indeterminate(FLAGS_OVERFLOW);
        }
        self
    }

    /// Makes the stream indeterminate. The first cause met is kept,
    /// except that a cause other than `Pending` replaces `Pending`
    /// (`spec/landed/digest/WP-024.md`).
    pub(crate) fn indeterminate(&mut self, cause: Cause) -> &mut Self {
        self.cause = Some(match self.cause {
            None => cause,
            Some(Cause::Pending(_)) if !matches!(cause, Cause::Pending(_)) => cause,
            Some(c) => c,
        });
        self
    }

    /// The cause of a nested stream whose digest this stream writes.
    pub(crate) fn absorb(&mut self, cause: Option<Cause>) -> &mut Self {
        if let Some(c) = cause {
            self.indeterminate(c);
        }
        self
    }

    /// An area of the walk that is not written yet (WP-024): a fixed
    /// placeholder, and the stream is indeterminate with `Pending(area)`.
    #[cfg(test)]
    pub(crate) fn pending(&mut self, area: Area) -> &mut Self {
        self.enc.uv(PENDING_PLACEHOLDER);
        self.indeterminate(Cause::Pending(area))
    }

    pub(crate) fn cause(&self) -> Option<Cause> {
        self.cause
    }

    /// The stream's identity (`walk::Walk::back_ref`).
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Counts a node written first in this stream: its count, from 0
    /// (`spec/digest.md` §4.3).
    pub(crate) fn count_node(&mut self) -> u64 {
        let n = self.nodes;
        self.nodes += 1;
        n
    }

    /// The bytes written so far.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.enc.as_bytes()
    }

    /// `H(tag, bytes)`. A traced stream an armed module made is kept
    /// when hashed as a unit ([`Armed`]).
    pub(crate) fn finish(self, tag: &str) -> Hashed {
        let bytes = self.enc.as_bytes();
        let h = Hashed {
            digest: tagged_digest(tag, bytes),
            cause: self.cause,
            bytes: bytes.len() as u64,
        };
        if self.rec.as_ref().is_some_and(|r| r.armed) && tag == UNIT_TAG {
            keep(self);
        }
        h
    }
}

/// Keeps the traced stream `s` an armed module made ([`Armed::take`]).
#[cold]
#[inline(never)]
fn keep(s: Stream) {
    let r = s.into_recording();
    KEPT.with(|k| k.borrow_mut().push(r));
}

/// The `bit` rows of one record, gathered in row order: the first is bit
/// 0 (`spec/digest-kinds.md` §2.1). No bit number is written by hand: the
/// position is the order of the pushes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FlagWord {
    bits: u64,
    n: u32,
}

/// A flag word given more than 64 rows: no record of the tables has as
/// many, so this is a shape the walk does not account for.
const FLAGS_OVERFLOW: Cause = Cause::Unaccounted("flags/overflow");

impl FlagWord {
    pub(crate) fn new() -> FlagWord {
        FlagWord::default()
    }

    /// The next `bit` row.
    #[must_use]
    pub(crate) fn bit(mut self, v: bool) -> FlagWord {
        // `n` saturates at 65: `flags` makes the stream indeterminate.
        if self.n < 64 {
            if v {
                self.bits |= 1 << self.n;
            }
            self.n += 1;
        } else {
            self.n = 65;
        }
        self
    }

    #[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
    pub(crate) fn value(self) -> u64 {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slip_model::wire::Encoder;

    fn bytes(f: impl FnOnce(&mut Stream)) -> Vec<u8> {
        let mut s = Stream::new();
        f(&mut s);
        s.as_bytes().to_vec()
    }

    fn enc(f: impl FnOnce(&mut Encoder)) -> Vec<u8> {
        let mut e = Encoder::new();
        f(&mut e);
        e.into_bytes()
    }

    // spec/digest.md §4: every primitive is model.md §2's encoding.
    #[test]
    fn primitives_are_the_wire_encodings() {
        for v in [0u64, 1, 127, 128, 300, u64::MAX] {
            assert_eq!(
                bytes(|s| {
                    s.uv(v);
                }),
                enc(|e| {
                    e.uv(v);
                }),
                "uv {v}"
            );
            assert_eq!(
                bytes(|s| {
                    s.count(v as usize);
                }),
                enc(|e| {
                    e.count(v as usize);
                })
            );
        }
        for v in [0i64, -1, 1, i64::MIN, i64::MAX] {
            assert_eq!(
                bytes(|s| {
                    s.sv(v);
                }),
                enc(|e| {
                    e.sv(v);
                }),
                "sv {v}"
            );
        }
        for v in [false, true] {
            assert_eq!(
                bytes(|s| {
                    s.bool(v);
                }),
                enc(|e| {
                    e.bool(v);
                })
            );
            assert_eq!(
                bytes(|s| {
                    s.opt(v);
                }),
                enc(|e| {
                    e.bool(v);
                })
            );
        }
        for v in [&b""[..], b"x", b"hello"] {
            assert_eq!(
                bytes(|s| {
                    s.bytes(v);
                }),
                enc(|e| {
                    e.bytes(v);
                })
            );
        }
        let d = Digest([7; 32]);
        assert_eq!(
            bytes(|s| {
                s.digest(&d);
            }),
            enc(|e| {
                e.digest(&d);
            })
        );
        let row = Row { tag: 1037, decision: crate::tables::Decision::In };
        assert_eq!(
            bytes(|s| {
                s.tag(row);
            }),
            enc(|e| {
                e.uv(1037);
            })
        );
    }

    // spec/digest-kinds.md §2.1: the first bit row is bit 0, in push order.
    #[test]
    fn flag_words_gather_bits_in_row_order() {
        assert_eq!(FlagWord::new().value(), 0);
        assert_eq!(FlagWord::new().bit(true).value(), 1);
        assert_eq!(FlagWord::new().bit(false).bit(true).value(), 2);
        assert_eq!(FlagWord::new().bit(true).bit(false).bit(true).value(), 5);
        let w = FlagWord::new().bit(false).bit(true).bit(true);
        assert_eq!(
            bytes(|s| {
                s.flags(w);
            }),
            enc(|e| {
                e.uv(6);
            })
        );
    }

    // spec/digest-kinds.md §1.3: `?` is a presence byte first, `*` a count
    // first, so two different trees give different bytes.
    #[test]
    fn optionals_and_lists_delimit_themselves() {
        // [[1], []] against [[], [1]]: the same elements, another tree.
        let a = bytes(|s| {
            s.count(2).count(1).uv(1).count(0);
        });
        let b = bytes(|s| {
            s.count(2).count(0).count(1).uv(1);
        });
        assert_ne!(a, b);
        // Some(None) against None followed by a 0: the presence bytes differ.
        let c = bytes(|s| {
            s.opt(true).opt(false);
        });
        let d = bytes(|s| {
            s.opt(false).uv(0);
        });
        assert_ne!(c, d);
        assert_eq!(
            bytes(|s| {
                s.opt(false);
            }),
            vec![0]
        );
        assert_eq!(
            bytes(|s| {
                s.opt(true).uv(5);
            }),
            vec![1, 5]
        );
        assert_eq!(
            bytes(|s| {
                s.count(2).uv(3).uv(4);
            }),
            vec![2, 3, 4]
        );
    }

    // spec/digest.md §10 and WP-024: the first cause is kept, a real cause
    // replaces Pending, and the digest is still the hash of the bytes.
    #[test]
    fn causes_and_the_hash() {
        let mut s = Stream::new();
        assert_eq!(s.cause(), None);
        s.pending(Area::Expr);
        assert_eq!(s.cause(), Some(Cause::Pending(Area::Expr)));
        s.pending(Area::Stmt);
        assert_eq!(s.cause(), Some(Cause::Pending(Area::Expr)));
        s.indeterminate(Cause::Error);
        assert_eq!(s.cause(), Some(Cause::Error));
        s.indeterminate(Cause::Collision);
        s.absorb(Some(Cause::Pending(Area::Value)));
        s.absorb(None);
        assert_eq!(s.cause(), Some(Cause::Error));
        assert_eq!(s.as_bytes(), &[0, 0]);
        let h = s.finish(UNIT_TAG);
        assert_eq!(h.digest, tagged_digest(UNIT_TAG, &[0, 0]));
        assert_eq!(h.bytes, 2);
        assert_ne!(h.digest, tagged_digest(TYPE_TAG, &[0, 0]));
    }
}
