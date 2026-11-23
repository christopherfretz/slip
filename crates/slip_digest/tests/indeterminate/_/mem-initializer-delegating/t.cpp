#include "side.h"
// A delegating constructor in a class template (and its target, whose list is outside the AST as well). Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S(int v) : m(v) { } S() : S(X) { } };
