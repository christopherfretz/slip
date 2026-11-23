#include "side.h"
// The class instantiated, the constructor declared only: the pattern alone is indeterminate. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() : m(X) { } };
int n = sizeof(S<int>);
