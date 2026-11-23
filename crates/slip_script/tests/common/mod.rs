//! What the driver's tests share (`docs/wp/WP-089.md`): transforms written
//! to a directory of their own under Cargo's temporary directory and named
//! by absolute path, loaded on a thread with the stack Kiln needs.

#![allow(dead_code, reason = "each test file uses a part")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use slip_kiln::session::Cancel;
use slip_model::Digest;
use slip_script::driver::{self, Budgets, LoadError, Loaded, Outcome, RunStats};
use slip_script::fake::{
    FakeCall, FakeDeclaration, FakeEntity, FakeExpression, FakeReference, FakeSubject,
};
use slip_script::subject::{Expression, Subject};

/// The stack of the thread a transform is compiled on.
pub const STACK: usize = 64 << 20;

/// The repository's resource directory (`spec/api.md` §2.1 rule 1).
pub fn resource() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource")).unwrap()
}

/// A new directory of this test process.
pub fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("driver-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

/// The transform file `<dir>/<name>.cpp` holding `text`, in a new
/// directory.
pub fn file(name: &str, text: &str) -> PathBuf {
    let path = scratch(name).join(format!("{name}.cpp"));
    std::fs::write(&path, text).unwrap();
    path
}

pub fn words(author: &[&str]) -> Vec<String> {
    author.iter().map(|&w| w.to_owned()).collect()
}

/// `driver::with_transform` on a thread with the stack it needs.
pub fn loading<R: Send>(
    resource: &Path,
    author: &[String],
    file: &Path,
    f: impl FnOnce(Result<Loaded<'_>, LoadError>) -> R + Send,
) -> R {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(s, || {
                driver::with_transform(resource, author, file, STACK - (1 << 20), f)
            })
            .unwrap()
            .join()
            .unwrap()
    })
}

/// What loading `text` (with the repository's resource directory and
/// `author`) gives: the error, or the transform id.
pub fn load(name: &str, text: &str, author: &[&str]) -> Result<Digest, LoadError> {
    let file = file(name, text);
    loading(&resource(), &words(author), &file, |l| l.map(|l| l.id()))
}

/// The error of §12 that loading `text` gives, by name.
pub fn load_error(name: &str, text: &str) -> &'static str {
    match load(name, text, &[]) {
        Ok(_) => panic!("{name}: the transform loads"),
        Err(e) => e.name(),
    }
}

/// `f` with the loaded transform `text`.
pub fn with<R: Send>(
    name: &str,
    text: &str,
    author: &[&str],
    f: impl FnOnce(&mut Loaded<'_>) -> R + Send,
) -> R {
    let file = file(name, text);
    loading(&resource(), &words(author), &file, |l| match l {
        Ok(mut l) => f(&mut l),
        Err(e) => panic!("{name}: the transform does not load: {e}"),
    })
}

/// One run with the default budgets and a fresh token.
pub fn run(l: &mut Loaded<'_>, s: &(dyn Subject + Sync)) -> Result<(Outcome, RunStats), String> {
    l.run(s, Budgets::default(), &Cancel::new())
}

/// The kind of a failed outcome, or `completed`.
pub fn kind(o: &Outcome) -> &'static str {
    match o {
        Outcome::Completed(_) => "completed",
        Outcome::Failed { kind, .. } => kind,
    }
}

/// The rendering of a failed outcome (empty for a completed one).
pub fn rendered(o: &Outcome) -> &str {
    match o {
        Outcome::Completed(_) => "",
        Outcome::Failed { rendered, .. } => rendered,
    }
}

/// `#include <slip/meta.h>` and `body`.
pub fn with_meta(body: &str) -> String {
    format!("#include <slip/meta.h>\n{body}\n")
}

/// The subject of `spec/api.md` §10.1: `lib::foo(int, int)` and
/// `lib::bar(int, int)`, and one call `lib::foo(1, 2)` in an owned file.
pub const SUBJECT: &str = "namespace lib { int foo(int a, int b); int bar(int a, int b); }\n\
int main() { return lib::foo(1, 2); }\n";

pub fn subject() -> FakeSubject {
    let mut f = FakeSubject::new();
    let g = FakeSubject::GLOBAL;
    let t = f.file("/src/t.cpp", SUBJECT, true);
    let lib = f.entity(FakeEntity::namespace("lib", g));
    let foo = f.entity(FakeEntity {
        key: Some(Digest([7; 32])),
        ..FakeEntity::function("foo", lib, Vec::new())
    });
    let fa = f.entity(FakeEntity::parameter("a", foo));
    let fb = f.entity(FakeEntity::parameter("b", foo));
    f.entities[foo.0 as usize].parameters = Some(vec![fa, fb]);
    let bar = f.entity(FakeEntity {
        key: Some(Digest([8; 32])),
        ..FakeEntity::function("bar", lib, Vec::new())
    });
    let ba = f.entity(FakeEntity::parameter("a", bar));
    let bb = f.entity(FakeEntity::parameter("b", bar));
    f.entities[bar.0 as usize].parameters = Some(vec![ba, bb]);
    let main = f.entity(FakeEntity::function("main", g, Vec::new()));
    let n = |f: &FakeSubject, needle, nth| Some(f.range_nth(t, needle, nth));
    for (entity, needle, nth) in
        [(lib, "lib", 0), (foo, "foo", 0), (bar, "bar", 0), (main, "main", 0)]
    {
        let name = n(&f, needle, nth);
        f.declaration(FakeDeclaration {
            entity,
            name,
            definition: entity == lib || entity == main,
        });
    }
    let callee = f.reference(FakeReference {
        name: n(&f, "foo", 1),
        extent: n(&f, "lib::foo", 0),
        ..FakeReference::to(foo, "foo")
    });
    let first = f.expressions.len() as u32;
    f.expression(FakeExpression {
        call: Some(FakeCall {
            callee: foo,
            reference: callee,
            arguments: vec![Expression(first + 1), Expression(first + 2)],
        }),
        unit_keys: Ok(vec![Digest([9; 32])]),
        ..FakeExpression::at(n(&f, "lib::foo(1, 2)", 0))
    });
    f.expression(FakeExpression::at(n(&f, "1", 0)));
    f.expression(FakeExpression::at(n(&f, "2", 0)));
    f
}
