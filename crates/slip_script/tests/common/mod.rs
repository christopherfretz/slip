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
use slip_script::subject::{
    Declaration, Entity, Expression, File, PlaceOf, Range, Reference, Subject, Value,
};
use slip_script::wire::Errc;

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
    let lib_foo = f.entity(FakeEntity {
        key: Some(Digest([7; 32])),
        ..FakeEntity::function("foo", lib, Vec::new())
    });
    let fa = f.entity(FakeEntity::parameter("a", lib_foo));
    let fb = f.entity(FakeEntity::parameter("b", lib_foo));
    f.entities[lib_foo.0 as usize].parameters = Some(vec![fa, fb]);
    let lib_bar = f.entity(FakeEntity {
        key: Some(Digest([8; 32])),
        ..FakeEntity::function("bar", lib, Vec::new())
    });
    let ba = f.entity(FakeEntity::parameter("a", lib_bar));
    let bb = f.entity(FakeEntity::parameter("b", lib_bar));
    f.entities[lib_bar.0 as usize].parameters = Some(vec![ba, bb]);
    let main = f.entity(FakeEntity::function("main", g, Vec::new()));
    let n = |f: &FakeSubject, needle, nth| Some(f.range_nth(t, needle, nth));
    for (entity, needle, nth) in
        [(lib, "lib", 0), (lib_foo, "foo", 0), (lib_bar, "bar", 0), (main, "main", 0)]
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
        ..FakeReference::to(lib_foo, "foo")
    });
    let first = f.expressions.len() as u32;
    f.expression(FakeExpression {
        call: Some(FakeCall {
            callee: lib_foo,
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

/// The fake subject, but for `lookup` of two names: `lie::lie` answers an
/// entity that is not live (the host's check of the answer fails: a host
/// failure, §2.8), and `boom::boom` panics.
pub struct Liar<'a>(pub &'a FakeSubject);

impl Subject for Liar<'_> {
    fn is_live(&self, v: Value) -> bool {
        self.0.is_live(v)
    }
    fn file_len(&self, f: File) -> Option<u32> {
        self.0.file_len(f)
    }
    fn kind_bits(&self, v: Value) -> u64 {
        self.0.kind_bits(v)
    }
    fn lookup(&self, global: bool, components: &[&[u8]]) -> Vec<Entity> {
        match components {
            [b"lie", b"lie"] => vec![Entity(9999)],
            [b"boom", b"boom"] => panic!("the subject panics"),
            _ => self.0.lookup(global, components),
        }
    }
    fn identifier(&self, v: Value) -> Option<Vec<u8>> {
        self.0.identifier(v)
    }
    fn display(&self, v: Value) -> String {
        self.0.display(v)
    }
    fn parent(&self, e: Entity) -> Option<Entity> {
        self.0.parent(e)
    }
    fn parameters(&self, v: Value) -> Option<Vec<Entity>> {
        self.0.parameters(v)
    }
    fn declarations(&self, e: Entity) -> Result<Vec<Declaration>, Errc> {
        self.0.declarations(e)
    }
    fn entity_of(&self, d: Declaration) -> Entity {
        self.0.entity_of(d)
    }
    fn origin(&self, e: Entity) -> Entity {
        self.0.origin(e)
    }
    fn references_to(&self, e: Entity) -> Vec<Reference> {
        self.0.references_to(e)
    }
    fn referent(&self, r: Reference) -> Entity {
        self.0.referent(r)
    }
    fn name_range(&self, v: Value) -> Result<Option<Range>, Errc> {
        self.0.name_range(v)
    }
    fn extent(&self, v: Value) -> Result<Option<Range>, Errc> {
        self.0.extent(v)
    }
    fn text(&self, r: Range) -> &[u8] {
        self.0.text(r)
    }
    fn owned(&self, f: File) -> bool {
        self.0.owned(f)
    }
    fn calls_to(&self, e: Entity) -> Result<Vec<Expression>, Errc> {
        self.0.calls_to(e)
    }
    fn callee(&self, x: Expression) -> Option<Entity> {
        self.0.callee(x)
    }
    fn callee_reference(&self, x: Expression) -> Option<Reference> {
        self.0.callee_reference(x)
    }
    fn arguments(&self, x: Expression) -> Option<Vec<Expression>> {
        self.0.arguments(x)
    }
    fn unresolved_named(&self, identifier: &[u8]) -> Option<u64> {
        self.0.unresolved_named(identifier)
    }
    fn tiers(&self, r: Range) -> Option<[bool; 2]> {
        self.0.tiers(r)
    }
    fn qualified(&self, e: Entity) -> Option<Vec<Vec<u8>>> {
        self.0.qualified(e)
    }
    fn references_at(&self, r: Range) -> Vec<Reference> {
        self.0.references_at(r)
    }
    fn found_directly(&self, r: Reference) -> bool {
        self.0.found_directly(r)
    }
    fn entity_key(&self, e: Entity) -> Option<Digest> {
        self.0.entity_key(e)
    }
    fn unit_keys(&self, v: Value) -> Result<Vec<Digest>, Errc> {
        self.0.unit_keys(v)
    }
    fn rename_rule(
        &self,
        e: Entity,
        new_identifier: &[u8],
    ) -> Result<slip_model::keys::Rule, Errc> {
        self.0.rename_rule(e, new_identifier)
    }
    fn place(&self, v: Value) -> PlaceOf {
        self.0.place(v)
    }
    fn file_ref(&self, f: File) -> slip_model::FileRef {
        self.0.file_ref(f)
    }
}

/// `subject()` with a namespace `mode` holding a function named `m`, so
/// that `lookup("mode::<m>")` finds it: a transform chooses what to do by
/// the subject it is given.
pub fn mode(m: &str) -> FakeSubject {
    let mut f = subject();
    let ns = f.entity(FakeEntity::namespace("mode", FakeSubject::GLOBAL));
    let e = f.entity(FakeEntity::function(m, ns, Vec::new()));
    f.declaration(FakeDeclaration { entity: ns, name: None, definition: true });
    f.declaration(FakeDeclaration { entity: e, name: None, definition: false });
    f
}
