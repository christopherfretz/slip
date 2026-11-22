# KD2 — a parenthesised `std::min` call panics the Oven's min/max fast path

Sent to the Kiln agent 2026-11-22. Found by the second run of
`slip parse` over the `llvm` corpus, after Slip's pin moved from
`4bfd999da` to `62d190007` (Kiln `63439c92c` plus the KF1 branch): 13
units that were `ok` now end in a panic of the front end, `expected a
call, found ParenExpr`. Introduced by Kiln `8af3bbf6a` (WP-1907/1917/
1918, the `std::min`/`std::max` fast path).

```cpp
namespace std {
template <class T> constexpr const T &min(const T &a, const T &b) { return b < a ? b : a; }
}
unsigned long long f(unsigned long n) { return 1ull << (std::min((unsigned long)31, n)); }
```

`-std=c++17`, no other option; clang 19 accepts it. Without the
parentheses around the call it is fine. The backtrace:
`ExprArena::expect_call` (`kiln_ast/src/expr.rs:2056`) from
`ExprArena::callee`, `kiln_oven::compile::regs::direct_callee`,
`Compiler::minmax_value` (`compile/fast.rs:200`), `Compiler::cast`,
`rv_inner`, `rv_into`, `compile_thunk`; reached from
`Sema::diagnose_shift_count` through `fold_int` and `try_fold`.
`cast` hands `minmax_value` its operand as the call, and the operand
is the `ParenExpr` around it.

In LLVM 19.1.7 the source is `llvm/include/llvm/ADT/ConcurrentHashtable.h:153`
(`1Ull << (std::min((size_t)31, LeadingZerosNumber))`). Units: of
`llvm/lib/DWARFLinker/Parallel/`: `AcceleratorRecordsSaver.cpp`, `DIEAttributeCloner.cpp`, `DWARFEmitterImpl.cpp`, `DWARFLinker.cpp`, `DWARFLinkerCompileUnit.cpp`, `DWARFLinkerImpl.cpp`, `DWARFLinkerTypeUnit.cpp`, `DWARFLinkerUnit.cpp`, `DependencyTracker.cpp`, `OutputSections.cpp`, `SyntheticTypeNameBuilder.cpp`; and `llvm/unittests/ADT/ConcurrentHashtableTest.cpp`, `llvm/unittests/DWARFLinkerParallel/StringPoolTest.cpp`.
