//! The `wp` lint (`spec/harness.md` §3): every `docs/wp/WP-*.md` follows
//! `docs/wp/README.md`, as [`crate::wp::parse`] reads it.

use super::Violation;
use crate::repo::Repo;
use crate::wp;

/// `wp`.
pub fn wp(repo: &Repo) -> Vec<Violation> {
    match wp::load(repo) {
        Ok(_) => Vec::new(),
        Err(problems) => problems,
    }
}
