#include "side.h"
// WP-124: the source of differ/_/mem-initializer-in-template, which stays pending on Kiln WP-3026: until then the pattern constructor is indeterminate, kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
template<int N> struct Bq { Bq(); Bq(int); };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { int m;
#ifdef B
  S() : m(2) { }
#else
  S() : m(1) { }
#endif
};
