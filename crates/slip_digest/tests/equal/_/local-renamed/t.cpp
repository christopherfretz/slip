#include "side.h"
// §4.3: a local variable, a parameter and a local class renamed.
#ifdef B
int f(int q) { struct M { int m; }; M w{q}; return w.m; }
#else
int f(int p) { struct L { int m; }; L v{p}; return v.m; }
#endif
