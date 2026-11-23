#include "side.h"
// WP-124 fix round F3: a local class in a lambda in a pattern; the cause lands on the unit that holds the lambda, a friend function's body (a local class, no lambda) (kiln/pattern-mem-initializers).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { friend void h(S) { struct L { int m; L() : m(X) {} }; (void)L(); } };
