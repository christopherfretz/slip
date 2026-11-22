#include "side.h"
// §10.1 AttrNode: `ANNOTATE.a`: the fact differs.
#ifdef B
[[clang::annotate("y")]] void f();
#else
[[clang::annotate("x")]] void f();
#endif
