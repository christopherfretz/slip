#include "side.h"
// A constructor with a function-try-block in a class template. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S() try : m(X) { } catch (...) { } };
