#include "side.h"
// §4.4 "Spellings": a found set in the explicit template arguments of an unresolved name.
template<class U> int h(U);
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(h<decltype(f(T()))>(t));
