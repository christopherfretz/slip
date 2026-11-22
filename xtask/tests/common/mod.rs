//! Fixture trees for the xtask tests: a minimal Slip-shaped repository that
//! every lint passes, made in a fresh directory under the system temporary
//! directory and removed on drop.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture {
    pub root: PathBuf,
}

/// The files of the passing fixture.
pub const BASE: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        "[workspace]\nmembers = [\n  \"crates/*\", # all crates\n  \"xtask\",\n]\n\
         exclude = [\"kiln\"]\n\n[workspace.dependencies]\n\
         kiln_ast = { path = \"kiln/crates/kiln_ast\" }\n\
         slip_kiln = { path = \"crates/slip_kiln\" }\n",
    ),
    (
        "crates/slip_kiln/Cargo.toml",
        "[package]\nname = \"slip_kiln\"\n\n[dependencies]\nslip_model.workspace = true\n\
         kiln_ast.workspace = true\n",
    ),
    ("crates/slip_kiln/src/lib.rs", "//! Adapter.\n\n#![forbid(unsafe_code)]\n"),
    (
        "crates/slip_digest/Cargo.toml",
        "[package]\nname = \"slip_digest\"\n\n[dependencies]\nslip_kiln = { workspace = true }\n",
    ),
    ("crates/slip_digest/src/lib.rs", "#![forbid(unsafe_code)]\n"),
    (
        "crates/slip_model/Cargo.toml",
        "[package]\nname = \"slip_model\"\n\n[dependencies]\n\n[dev-dependencies]\n",
    ),
    ("crates/slip_model/src/lib.rs", "#![forbid(unsafe_code)]\npub mod x;\n"),
    ("crates/slip_model/src/x.rs", "// not a crate root\n"),
    ("crates/slip_model/src/bin/tool.rs", "#![forbid(unsafe_code)]\nfn main() {}\n"),
    (
        "crates/slip_cdb/Cargo.toml",
        "[package]\nname = \"slip_cdb\"\n\n[dependencies.slip_model]\nworkspace = true\n",
    ),
    ("crates/slip_cdb/src/lib.rs", "#![forbid(unsafe_code)]\n"),
    ("xtask/Cargo.toml", "[package]\nname = \"xtask\"\n\n[dependencies]\n"),
    ("xtask/src/main.rs", "#![forbid(unsafe_code)]\nfn main() {}\n"),
    (
        "Cargo.lock",
        "version = 4\n\n[[package]]\nname = \"hashbrown\"\nversion = \"0.15.5\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n\
         [[package]]\nname = \"signal-hook\"\nversion = \"0.3.18\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n\
         [[package]]\nname = \"slip_model\"\nversion = \"0.0.0\"\n",
    ),
    (
        "kiln/Cargo.lock",
        "version = 4\n\n[[package]]\nname = \"hashbrown\"\nversion = \"0.15.5\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
    ),
    (
        "spec/README.md",
        "# Specs\n\n## Index\n\n| Spec | Defines | Status |\n|---|---|---|\n\
         | [model.md](model.md) | The model | draft |\n\
         | digest.md | The digest | not written (M1) |\n\n## Conventions\n\n\
         | Not | the index |\n|---|---|\n| x.md | nonsense |\n",
    ),
    ("spec/model.md", "# model\n\n## 1. Things\n\n## Tests\n\n## Open questions\n"),
    ("spec/landed/model/WP-001.md", "# WP-001 — Wire encoding\n\nText.\n"),
    (
        "docs/wp/WP-001.md",
        "# WP-001 — slip_model: wire\n\n- Status: done\n- Milestone: M0\n- Track: A\n\
         - Needs: none\n- Size: M\n- Parallel with: WP-002\n\n## Scope\n",
    ),
    (
        "docs/wp/WP-002.md",
        "# WP-002 — slip_cdb: split\n\n- Status: todo\n- Milestone: M0\n- Track: D\n\
         - Needs: WP-001\n- Size: S\n\n## Scope\n",
    ),
    ("docs/wp/README.md", "# Work packages\n\nNot a work package: [WP-001](WP-001.md).\n"),
    (
        "README.md",
        "# Slip\n\n[specs](spec/README.md), [wp](docs/wp/), [web](https://example.org/x),\n\
         `[code](nowhere.md)`, [anchor](#slip), [m](mailto:a@b.c), ![i](spec/model.md \"t\")\n\
         \n```\n[fenced](nowhere.md)\n```\n\n[ref]: docs/wp/README.md\n",
    ),
    (
        "docs/notes.md",
        "See [the model](../spec/model.md#tests) and [<wp>](<wp/WP-001.md>).\n\
         The rule names `/home/`, `/Users/`, `/root/` and `/Volumes/` bare; a/root/b and \
         <sysroot>/root/x are not local.\n",
    ),
];

impl Fixture {
    /// The passing fixture, in a new directory.
    pub fn new(tag: &str) -> Fixture {
        let f = Fixture::empty(tag);
        for (path, text) in BASE {
            f.write(path, text);
        }
        f
    }

    /// An empty new directory.
    pub fn empty(tag: &str) -> Fixture {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("slip-xtask-{tag}-{}-{n}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        std::fs::remove_file(self.root.join(rel)).unwrap();
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
