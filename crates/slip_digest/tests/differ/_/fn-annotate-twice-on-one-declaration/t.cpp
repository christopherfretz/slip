#include "side.h"
// §10.1 (WP-100): a function's `annotate` nodes are kept per declaration: once on each of two declarations (Kiln's `ann_attrs` merges them by string, one global annotation) against twice on one (two).
#ifdef B
[[clang::annotate("a")]] [[clang::annotate("a")]] void f();
void f() {}
#else
[[clang::annotate("a")]] void f();
[[clang::annotate("a")]] void f() {}
#endif
