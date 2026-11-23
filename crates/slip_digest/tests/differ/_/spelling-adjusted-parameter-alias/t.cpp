#include "side.h"
// As `spelling-adjusted-parameter`, through a member alias (s09).
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { using R = decltype(f(T())); void k(R[2]); };
