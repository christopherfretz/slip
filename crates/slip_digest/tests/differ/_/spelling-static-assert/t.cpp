#include "side.h"
// §4.4 "Spellings": a static_assert is content-keyed; the found set in a type its condition writes is in its list, so its content (and key) moves.
int f(int);
template<unsigned long N> struct Z {};
#ifdef B
long f(long);
#endif
template<class T> struct S { static_assert(sizeof(Z<sizeof(f(T()))>) > 0); };
