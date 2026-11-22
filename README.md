# Slip

Slip is the code transformer of the **Mason** toolchain (Kiln: compiler front
end, Hod: code generator, Mortar: linker, Tuck: debugger).

It is a semantically aware refactoring and large-scale code transformation
tool for C++. Transformations are written in general-purpose constexpr C++
using reflection and Slip-specific intrinsics, and run on top of Kiln's
preprocessor, parser, AST and semantic analysis.

Goals, in priority order:

1. **Correct** — a transformation must preserve the meaning it promises to
   preserve, and Slip must be able to validate that.
2. **Memory safe** — written in safe Rust.
3. **Programmable and extensible** — transformations are reusable, composable
   programs with a stable API.
4. **Fast and lean** — scales to codebases of tens of millions of lines, across
   many cores, with wall-clock time and memory tracked as part of validation.

## Status

Design phase. Nothing here is usable yet. The end-to-end design lives under
`docs/` once written.
