#include "side.h"
// A constructor of a class template defined outside the class: the rule reads the definition, not the in-class declaration. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S(); };
template<class T> S<T>::S() : m(X) { }
