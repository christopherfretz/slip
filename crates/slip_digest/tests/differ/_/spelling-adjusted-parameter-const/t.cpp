#include "side.h"
// As `spelling-adjusted-parameter`, a qualified element (r006).
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, const decltype(f(T()))[3]);
