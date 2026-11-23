#include "side.h"
// digest §4.3 back-references, `backref-range-inside`: a designator range writes its clause once per slot; a term inside the shared clause differs.
int g(int);
#ifdef B
long v[4] = {[0 ... 3] = g(1) + g(2) + g(3)};
#else
long v[4] = {[0 ... 3] = g(1) + g(5) + g(3)};
#endif
