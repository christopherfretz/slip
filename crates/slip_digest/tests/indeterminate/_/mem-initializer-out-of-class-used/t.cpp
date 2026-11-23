#include "side.h"
// As mem-initializer-out-of-class, instantiated and defined. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S(); };
template<class T> S<T>::S() : m(X) { }
S<int> s;
