#include "side.h"
// §4.4 "Spellings": a found set in a default template argument.
int f(int);
template<class T, class U = decltype(f(T()))> struct G;
#ifdef B
long f(long);
#endif
template<class T, class U = decltype(f(T()))> struct H;
