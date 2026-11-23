#include "side.h"
// §4.4 "Spellings": a found set in the template arguments of a variable template's name.
template<class U> constexpr int v = 0;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(v<decltype(f(T()))> + t);
