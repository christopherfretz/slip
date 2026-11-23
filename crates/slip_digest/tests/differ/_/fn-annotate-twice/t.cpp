#include "side.h"
// §10.1 (WP-100): a function's `annotate` nodes are kept with their multiplicity: twice against once on one declaration.
#ifdef B
[[clang::annotate("a")]] [[clang::annotate("a")]] void f() {}
#else
[[clang::annotate("a")]] void f() {}
#endif
