#include "side.h"
// Removing the mem-initializer lets the default member initialiser apply: a change only the list shows. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
template<class T> struct S { int m = 3;
#ifdef B
  S() { }
#else
  S() : m(1) { }
#endif
};
