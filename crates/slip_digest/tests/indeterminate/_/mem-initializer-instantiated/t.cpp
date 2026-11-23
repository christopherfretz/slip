#include "side.h"
// The pattern constructor is indeterminate; the instantiation S<int>::S() and its definition hold their own list and are not. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() : m(X) { } };
S<int> s;
