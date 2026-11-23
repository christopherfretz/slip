#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the first use of a function template names the typedef or `int`: Kiln keeps the first use's sugar in the specialization's arguments, but `layout_desugar_once` reads it canonical, so `alignof(decltype(x))` is 4 either way (Kiln's tests/conformance/expr.alignof/run-aligned-typedef-template-argument.cpp; clang 19: 4).
typedef int I __attribute__((aligned(8)));
template <class T> unsigned long f() { T x = 0; return alignof(decltype(x)); }
#ifdef B
unsigned long a = f<I>();
unsigned long b = f<int>();
#else
unsigned long a = f<int>();
unsigned long b = f<I>();
#endif
