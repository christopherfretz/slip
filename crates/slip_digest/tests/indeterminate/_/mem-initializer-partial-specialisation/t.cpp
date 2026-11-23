#include "side.h"
// A constructor of a class template partial specialisation. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S;
template<class T> struct S<T*> { int m; S() : m(X) { } };
