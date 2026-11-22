//! The walk over a finished unit of the parse task (`spec/model.md` §8.5):
//! the map task's use of [`digest`](crate::digest), and the checks every
//! use of the walk on a [`slip_kiln::Unit`] shares. Work package WP-050.
//!
//! - **The stack.** The walk needs [`STACK_SIZE`] bytes of stack
//!   (`lib.rs` "What bounds the stack"). [`with_input`] compares
//!   `unit.stack_remaining()` with it before anything else: less, or no
//!   stack guard at all, is `Err`, so that a command whose
//!   `-fkiln-stack-size=` is too small fails its job instead of
//!   overflowing.
//! - **Subject paths.** A buffer that is one of the shard's `inputs` has
//!   `subject(real path)`; a buffer that is not an input and whose name
//!   starts with `<` (the predefines `<built-in>`, the command line's
//!   macros) has that name; any other buffer the walk asks for is `Err`
//!   (`a declaration lies in <name>, which is not an input`). The walk
//!   cannot fail, so the first such buffer is kept in a cell and returned
//!   once the walk is done; its result is then dropped.
//! - **Counters.** [`counters`] gives the walk's counters as a shard
//!   carries them: `digest.md` §12, `digest.indeterminate.<cause>` (from
//!   the walk), and `digest.cause.<cause>.<word>` for the causes with a
//!   text, `<word>` from [`cause_word`].

use std::cell::RefCell;
use std::collections::BTreeMap;

use slip_kiln::Unit;
use slip_kiln::kiln::source::FileId;
use slip_model::Counter;

use crate::{Cause, Input, Options, Output, STACK_SIZE, digest};

/// The walk over a finished unit. `subject` maps the real path of an
/// input to its subject path (`spec/verify.md` §5.1); the identity for the
/// map phase.
///
/// `with_input(unit, subject, |input| digest(input, options))`.
///
/// # Errors
///
/// As [`with_input`].
pub fn digest_unit(
    unit: &Unit<'_>,
    options: &Options<'_>,
    subject: &dyn Fn(&[u8]) -> Vec<u8>,
) -> Result<Output, String> {
    with_input(unit, subject, |input| digest(input, options))
}

/// Calls `f` on the [`Input`] of a finished unit, after the checks of
/// `spec/model.md` §8.5 "The stack", for any use of the walk on it
/// (`spec/verify.md` §2.3 records, §6.1 traces); `subject` as for
/// [`digest_unit`].
///
/// # Errors
///
/// - `the front end's thread has <n> bytes of stack left; the digest
///   needs <STACK_SIZE>`, or `no stack guard on the front end's thread`:
///   `f` is not called.
/// - `a declaration lies in <name>, which is not an input`: the first
///   buffer `f` asked the subject path of that is neither an input nor
///   named `<…>`. `f` ran; its result is dropped.
pub fn with_input<R>(
    unit: &Unit<'_>,
    subject: &dyn Fn(&[u8]) -> Vec<u8>,
    f: impl FnOnce(&Input<'_>) -> R,
) -> Result<R, String> {
    match unit.stack_remaining() {
        None => return Err("no stack guard on the front end's thread".to_owned()),
        Some(left) if left < STACK_SIZE => {
            return Err(format!(
                "the front end's thread has {left} bytes of stack left; the digest needs \
                 {STACK_SIZE}"
            ));
        }
        Some(_) => {}
    }
    let sm = unit.sm();
    let not_an_input: RefCell<Option<String>> = RefCell::new(None);
    let subject_path = |file: FileId| -> Vec<u8> {
        if let Some(real) = unit.real_path(file) {
            return subject(real);
        }
        let name = sm.file_name(file);
        if name.starts_with('<') {
            return name.as_bytes().to_vec();
        }
        let mut first = not_an_input.borrow_mut();
        if first.is_none() {
            *first = Some(format!("a declaration lies in {name}, which is not an input"));
        }
        Vec::new()
    };
    let input = Input { ast: unit.ast(), sm, subject_path: &subject_path };
    let value = f(&input);
    match not_an_input.into_inner() {
        Some(e) => Err(e),
        None => Ok(value),
    }
}

/// The counters of a walk as a shard carries them (`spec/model.md` §8.5
/// "Counters"): `output.counters`, and for each indeterminate key whose
/// cause carries a text (`unsupported`, `unaccounted`) one count of
/// `digest.cause.<cause>.<word>`, `<word>` being [`cause_word`] of the
/// text; two texts with one word add up. Sorted by name.
pub fn counters(output: &Output) -> Vec<Counter> {
    let mut by_name: BTreeMap<String, u64> = BTreeMap::new();
    for c in &output.counters {
        *by_name.entry(c.name.clone()).or_default() += c.value;
    }
    for (_, cause) in &output.causes {
        if let Cause::Unsupported(text) | Cause::Unaccounted(text) = cause {
            let name = format!("digest.cause.{}.{}", cause.name(), cause_word(text));
            *by_name.entry(name).or_default() += 1;
        }
    }
    by_name.into_iter().map(|(name, value)| Counter { name, value }).collect()
}

/// The counter word of a cause's text (`spec/model.md` §8.5): ASCII
/// upper-case letters made lower-case, `/` made `.`, every other byte
/// outside `[a-z0-9_]` made `_`, and an empty word (between two `.`, or
/// at either end) made `_`. `ExprKind/CXXReflectExpr` is
/// `exprkind.cxxreflectexpr`; the empty text is `_`.
pub fn cause_word(text: &str) -> String {
    let mapped: String = text
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' => char::from(b.to_ascii_lowercase()),
            b'a'..=b'z' | b'0'..=b'9' | b'_' => char::from(b),
            b'/' => '.',
            _ => '_',
        })
        .collect();
    mapped.split('.').map(|w| if w.is_empty() { "_" } else { w }).collect::<Vec<_>>().join(".")
}
