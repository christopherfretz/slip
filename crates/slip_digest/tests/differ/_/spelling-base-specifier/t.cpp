#include "side.h"
// §4.4 "Spellings": a found set in a base specifier.
int f(int);
template<unsigned long N> struct Z {};
template<class T> struct G : Z<sizeof(f(T()))> {};
#ifdef B
long f(long);
#endif
template<class T> struct H : Z<sizeof(f(T()))> {};
