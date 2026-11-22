#include "side.h"
// §5.6 ScopeData `aux-braceless` (out, WP-033 review): for a function a linkage specification with or without braces means the same.
#ifdef B
extern "C" void f(); extern "C" int g(int);
#else
extern "C" { void f(); int g(int); }
#endif
