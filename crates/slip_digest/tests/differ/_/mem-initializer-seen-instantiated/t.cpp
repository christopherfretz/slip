#include "side.h"
// WP-124 fix round F1: the pattern constructor is indeterminate (kiln/pattern-mem-initializers); the change is seen through the instantiation, whose units are determinate.
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() : m(X) { } };
S<int> s;
