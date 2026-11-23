#include "side.h"
// A local class of a function template: its constructor is in the function template's unit. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> void f() { struct L { int m; L() : m(X) { } }; L l; }
