#include "side.h"
// digest §4.3: the first copy of a closure; `mk` does not change.
auto mk() { return [] { return 1; }; }
int a() { return mk()(); }
#ifdef B
int c() { auto x = mk(); auto y = x; return y(); }
#endif
