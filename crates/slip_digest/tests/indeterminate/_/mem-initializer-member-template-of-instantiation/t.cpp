#include "side.h"
// A member class template of a class template whose specialisation is instantiated: A<int>::C<U> is still a pattern. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct A { template<class U> struct C { int m; C() : m(X) { } }; };
A<int> a;
