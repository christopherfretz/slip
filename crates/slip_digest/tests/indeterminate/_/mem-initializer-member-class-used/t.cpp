#include "side.h"
// As mem-initializer-member-class, instantiated and defined: O<int>::I::I() is determinate. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct O { struct I { int m; I() : m(X) { } }; };
O<int>::I i;
