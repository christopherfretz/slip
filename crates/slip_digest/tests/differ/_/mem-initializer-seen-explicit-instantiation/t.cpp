#include "side.h"
// WP-124 fix round F1: as mem-initializer-seen-instantiated, through an explicit instantiation definition.
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() : m(X) { } };
template struct S<int>;
