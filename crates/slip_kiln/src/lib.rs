//! The Slip/Kiln boundary (`docs/design/05-kiln-interface.md`).
//!
//! This is the only Slip crate that links Kiln. It drives Kiln's front end
//! over a normalised compile command and turns what Kiln produced into
//! Slip's own model types (`slip_model`); no Kiln type appears in its public
//! interface, so a Kiln API change touches this crate alone.

#![forbid(unsafe_code)]

mod check;
mod depfile;
mod diags;
mod parse;

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
