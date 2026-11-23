//! `Stream`: the only way bytes reach a hash (`spec/digest.md` §4).
//!
//! A stream is the bytes of one unit, type, discriminator or key, in the
//! primitives of `spec/model.md` §2, together with the first reason it
//! cannot be trusted (`spec/digest.md` §10). Nested streams (a type inside
//! a unit) hand their cause to the stream that writes their digest
//! ([`Stream::absorb`]).

use slip_model::Digest;
use slip_model::hash::tagged_digest;
use slip_model::wire::Encoder;

#[cfg(test)]
use crate::Area;
use crate::Cause;
use crate::tables::Row;

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
}

thread_local! {
    /// The identity the next stream of this thread takes. A walk runs on
    /// one thread and its memo of back-references lives no longer than
    /// it does, so identities unique per thread are enough.
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl Default for Stream {
    fn default() -> Stream {
        let id = NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        Stream { enc: Encoder::default(), cause: None, id, nodes: 0 }
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

    /// `H(tag, bytes)`.
    pub(crate) fn finish(self, tag: &str) -> Hashed {
        let bytes = self.enc.as_bytes();
        Hashed { digest: tagged_digest(tag, bytes), cause: self.cause, bytes: bytes.len() as u64 }
    }
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
