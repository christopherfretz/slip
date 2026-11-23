#include "side.h"
// §4.4 "Spellings": a found set in a non-type argument of a specialisation used as a parameter type.
int f(int);
template<unsigned long N> struct Z {};
template<class T> void g(T t, Z<sizeof(f(T()))> z);
#ifdef B
long f(long);
#endif
template<class T> void h(T t, Z<sizeof(f(T()))> z);
