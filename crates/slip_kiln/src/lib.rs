//! The Slip/Kiln boundary (`docs/design/05-kiln-interface.md`).
//!
//! This is the only Slip crate that links Kiln. It drives Kiln's front end
//! over a normalised compile command and turns what Kiln produced into
//! Slip's own model types (`slip_model`). No Kiln type appears in its public
//! interface outside the module [`kiln`], so a Kiln API change touches this
//! crate and the crates that are allowed to name that module, and no other.

#![forbid(unsafe_code)]

mod check;
mod depfile;
mod diags;
mod parse;

/// Kiln's own crates, for the Slip crates whose subject is Kiln's AST.
///
/// The semantic digest is specified field by field against Kiln's AST
/// (`spec/digest-kinds.md`), so `slip_digest` has to name Kiln's types; it
/// names them through here, and this crate's manifest stays the only one
/// that names a `kiln_*` crate (`spec/harness.md` §3, `kiln-boundary`).
/// Only `slip_digest` and `slip_script` may use this module.
pub mod kiln {
    pub use kiln_ast as ast;
    pub use kiln_snap as snap;
    pub use kiln_source as source;
}

pub use check::KilnCheck;
pub use parse::{ParseOptions, parse_observer};

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
