//! The Slip/Kiln boundary (`docs/design/05-kiln-interface.md`).
//!
//! This is the only Slip crate that links Kiln. It drives Kiln's front end
//! over a normalised compile command and turns what Kiln produced into
//! Slip's own model types (`slip_model`).
//!
//! **No Kiln type outside [`kiln`], [`Unit`]'s methods, [`ranges`],
//! [`sites`], [`snippet`] and [`written`].**
//! The public interface is:
//!
//! - the parse task (`spec/model.md` §8.4): [`parse_observer`] (an
//!   observer to its shard, through Kiln's front-end library entry
//!   `kiln_driver::front_end::run_front_end`) and [`parse_observer_with`]
//!   (the same, with a closure called on the finished [`Unit`] before
//!   anything is dropped: the production entry of the semantic digest),
//!   whose [`ParseOptions`] ask for what the front end keeps only on
//!   request (the preprocessing record, the written-reference and
//!   declaration-extent tables);
//! - [`KilnCheck`] (`spec/cdb.md` §6) and [`kiln_version`];
//! - the module [`session`]: the transform unit compiled once and run
//!   many times (`spec/api.md` §2), whose types name no Kiln type;
//! - the module [`kiln`] (re-exports of Kiln's crates), which [`Unit`]'s
//!   methods name, the module [`ranges`] (the file range of a token span
//!   and its tier, and unit order: `spec/api.md` §1 and §5.4, over Kiln's
//!   locations) and the module [`snippet`] (an in-memory front end for
//!   tests, whose types name Kiln's);
//! - the module [`written`]: a unit's names as written, from Kiln's
//!   written-reference and declaration-extent tables (`spec/edits.md`
//!   §9.7), whose types name Kiln's spans and declarations;
//! - the module [`sites`]: where a name token is written and why it
//!   cannot be edited (`spec/edits.md` §2.2, §2.3), over Kiln's spans and
//!   buffer ids; the module [`scan`]: the lexical half of the residue scan
//!   (§8.4), Kiln's lexer over bytes, whose types name no Kiln type; the
//!   module [`tree`]: a front end over files written to a fresh directory,
//!   for tests.
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
pub mod ranges;

/// Kiln's own crates, for the Slip crates whose subject is Kiln's AST.
///
/// The semantic digest is specified field by field against Kiln's AST
/// (`spec/digest-kinds.md`), so `slip_digest` has to name Kiln's types; it
/// names them through here, and this crate's manifest stays the only one
/// that names a `kiln_*` crate (`spec/harness.md` §3, `kiln-boundary`).
/// Only `slip_digest` and `slip_script` may use this module. `oven` is
/// there for [`snippet::Parsed::oven`] and [`Unit::oven`]; `pp` for the
/// preprocessing record ([`Unit::pp_record`], Kiln's `kiln_pp::PpRecord`)
/// and `lex` for `kiln_lex::token_end` (the end of a token), so that the
/// packages reading them call Kiln's functions and take no Kiln
/// dependency of their own (`docs/wp/WP-101.md`).
pub mod kiln {
    pub use kiln_ast as ast;
    pub use kiln_lex as lex;
    pub use kiln_oven as oven;
    pub use kiln_pp as pp;
    pub use kiln_snap as snap;
    pub use kiln_source as source;
}

pub mod scan;
pub mod session;
pub mod sites;
pub mod snippet;
pub mod tree;
pub mod written;

pub use check::KilnCheck;
pub use parse::{ParseOptions, Tokens, Unit, parse_observer, parse_observer_with};

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
