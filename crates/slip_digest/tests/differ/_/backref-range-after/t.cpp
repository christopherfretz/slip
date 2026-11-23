#include "side.h"
// digest §4.3 back-references, `backref-range-after`: the shared clause is the same; an initializer after the range differs.
int g(int);
#ifdef B
long v[5] = {[0 ... 3] = g(1) + g(2), [4] = g(3)};
#else
long v[5] = {[0 ... 3] = g(1) + g(2), [4] = g(4)};
#endif
