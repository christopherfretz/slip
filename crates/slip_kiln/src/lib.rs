//! The Slip/Kiln boundary (`docs/design/05-kiln-interface.md`).
//!
//! This is the only Slip crate that links Kiln. It drives Kiln's front end
//! over a normalised compile command and turns what Kiln produced into
//! Slip's own model types (`slip_model`).
//!
//! **No Kiln type outside [`kiln`], [`Unit`]'s methods and [`snippet`].**
//! The public interface is:
//!
//! - the parse task (`spec/model.md` §8.4): [`parse_observer`] (an
//!   observer to its shard, through Kiln's front-end library entry
//!   `kiln_driver::front_end::run_front_end`) and [`parse_observer_with`]
//!   (the same, with a closure called on the finished [`Unit`] before
//!   anything is dropped: the production entry of the semantic digest);
//! - [`KilnCheck`] (`spec/cdb.md` §6) and [`kiln_version`];
//! - the module [`session`]: the transform unit compiled once and run
//!   many times (`spec/api.md` §2), whose types name no Kiln type;
//! - the module [`kiln`] (re-exports of Kiln's crates), which [`Unit`]'s
//!   methods name, and the module [`snippet`] (an in-memory front end for
//!   tests, whose types name Kiln's).
//!
//! So a Kiln API change touches this crate and the crates that may name
//! those modules, and no other. Outside this crate only `slip_digest` and
//! `slip_script` may name them (and so call [`Unit`]'s methods; any crate
//! may pass a `&Unit` on), and `snippet` also the tests of the crates that
//! link Kiln (`spec/harness.md` §3 `kiln-boundary`,
//! `spec/landed/harness/WP-021.md`).

#![forbid(unsafe_code)]

mod check;
mod parse;

/// Kiln's own crates, for the Slip crates whose subject is Kiln's AST.
///
/// The semantic digest is specified field by field against Kiln's AST
/// (`spec/digest-kinds.md`), so `slip_digest` has to name Kiln's types; it
/// names them through here, and this crate's manifest stays the only one
/// that names a `kiln_*` crate (`spec/harness.md` §3, `kiln-boundary`).
/// Only `slip_digest` and `slip_script` may use this module. `oven` is
/// there for [`snippet::Parsed::oven`] and [`Unit::oven`].
pub mod kiln {
    pub use kiln_ast as ast;
    pub use kiln_oven as oven;
    pub use kiln_snap as snap;
    pub use kiln_source as source;
}

pub mod session;
pub mod snippet;

pub use check::KilnCheck;
pub use parse::{Unit, parse_observer, parse_observer_with};

/// The version of the Kiln front end this build of Slip links
/// (`MAJOR.MINOR.PATCH[-PRE]`).
pub fn kiln_version() -> &'static str {
    kiln_driver::version::VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_kiln() {
        let v = super::kiln_version();
        assert!(v.split('.').count() >= 3, "unexpected Kiln version {v:?}");
    }
}
