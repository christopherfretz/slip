#include "side.h"
// §10.1 AttrNode: `ANNOTATE.b`: the fact differs.
#ifdef B
[[clang::annotate("x", 1)]] void f();
#else
[[clang::annotate("x")]] void f();
#endif
