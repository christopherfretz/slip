#include "side.h"
// §4.4 "Spellings": a found set in a partial specialisation's arguments, read as written.
int f(int);
template<class T, unsigned long N> struct G;
template<class T, unsigned long N> struct H;
template<class T> struct G<T, sizeof(f(T()))> {};
#ifdef B
long f(long);
#endif
template<class T> struct H<T, sizeof(f(T()))> {};
