# KS1 amendment 1 — the handler of a run and the thread it lives on

A request from Slip to Kiln. Status: **sent 2026-11-22.**
Amends [KS1](KS1-host-call.md). Found while specifying `spec/api.md` §3
against Kiln main `0295475a3`; the reading is in
[kiln-landed-2026-11.md](../design/notes/kiln-landed-2026-11.md) §3.4.

## Design or implementation

| Part | Kind |
|---|---|
| Everything in this amendment | **Implementation against a settled specification.** It changes a trait bound or adds a compile-time assertion. No behaviour of the builtin changes. |

There is no design part.

## 1. Need

Slip's handler for one run answers questions about the subject unit, so it
borrows the subject: the syntax tree and the source manager of a
*different* Kiln instance than the one that holds the session.

`EvalSession::run` takes `host: &mut dyn HostCalls`
(`kiln/crates/kiln_sema/src/session.rs:486-492`), and the trait is declared
`pub trait HostCalls: Send` (`kiln/crates/kiln_oven/src/host.rs:13`), so
that an Oven holding a *registered* handler can move between threads
(`kiln/spec/host-calls.md` §3). A type that holds `&Ast` is `Send` only if
`Ast: Sync`. Kiln asserts `SourceManager: Send + Sync`
(`kiln/crates/kiln_source/src/manager.rs:1614-1615`) and asserts nothing
about `Ast`.

So today Slip's handler either does not compile, or compiles by an
accident that the next interior-mutable field in the tree would undo.

## 2. Request (either one)

**(a) The handler of a run need not be `Send`.** A run's handler is
borrowed for the call and never stored in the Oven
(`kiln/spec/eval-session.md` §6): it cannot outlive the call or change
thread. Split the trait so that only a registered handler carries the
bound, for example

```rust
pub trait HostCalls { fn call(&mut self, ...) -> Result<u64, HostError>; }
// registration:  Oven::set_host_calls(Box<dyn HostCalls + Send>)
// a run:         EvalSession::run(.., host: &mut dyn HostCalls, ..)
```

**(b) `Ast: Sync` is promised.** A compile-time assertion beside the one
for the source manager, and a line in `kiln/spec/ast.md` that the finished
tree of a translation unit may be read from several threads.

Slip prefers (a): it asks nothing of the tree. (b) is also useful on its
own, later, for reading one subject unit from several threads.

## 3. Not requested

A handler that may hold the subject's Oven. No query of API version 0
evaluates anything in the subject. When one does
(`constant_value_of`), Slip expects to give the handler ownership of the
subject unit for the run; that needs nothing from Kiln if (a) is done.

## 4. Acceptance

A test in Kiln that runs a session entry with a handler holding an
`Rc<Cell<u32>>` (for (a)), or a `static_assertions`-style check that
`Ast: Sync` (for (b)).
