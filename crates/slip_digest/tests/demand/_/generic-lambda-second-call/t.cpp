#include "side.h"
// digest §4.3: a second call of a generic lambda with another type makes a
// specialisation of its call operator, a derived unit under the closure;
// `mk`, whose stream holds the closure, does not change.
auto mk() { return [](auto x) { return x; }; }
int a() { return mk()(1); }
#ifdef B
long b() { return mk()(2L); }
#endif
