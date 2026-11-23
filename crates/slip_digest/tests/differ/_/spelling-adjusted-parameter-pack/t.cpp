#include "side.h"
// As `spelling-adjusted-parameter`, a pack of arrays (s08).
int f(int);
#ifdef B
long f(long);
#endif
template<class... T> struct S { void k(T..., decltype(f(T()))... [2]); };
