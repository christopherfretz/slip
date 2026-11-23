#include "side.h"
// No ctor-initializer: the AST keeps no mark of whether one was written, so this constructor is indeterminate too (the cost of the rule). Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() { m = X; } };
