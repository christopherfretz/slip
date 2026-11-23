#include "side.h"
// A constructor of a member class of a class template. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct O { struct I { int m; I() : m(X) { } }; };
