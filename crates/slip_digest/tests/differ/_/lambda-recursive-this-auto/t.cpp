// std: c++23
#include "side.h"
// §4.3: a recursive lambda through `this auto`.
#ifdef B
int f() { auto l = [](this auto self, int n) -> int { return n ? self(n - 2) : 0; }; return l(3); }
#else
int f() { auto l = [](this auto self, int n) -> int { return n ? self(n - 1) : 0; }; return l(3); }
#endif
