#include "side.h"
// §4.4 "Spellings", rule 11, audit (c): an overload set with explicit template arguments under `&`: side B adds `f(long)`, which the found set of `f` there does not hold.
template<int N> int h(int);
template<int N> int h(double);
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t) { int (*q)(int) = &h<sizeof(sizeof(f(t)))>; (void)q; }
