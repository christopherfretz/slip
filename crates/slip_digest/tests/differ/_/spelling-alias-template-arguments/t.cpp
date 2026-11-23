#include "side.h"
// §4.4 "Spellings", rule 4: an alias template specialisation's arguments are the user's spelling and are read; the alias itself holds no set.
int f(int);
template<class T> using Id = T;
template<class T> void g(T t, Id<decltype(f(T()))> x);
#ifdef B
long f(long);
#endif
template<class T> void h(T t, Id<decltype(f(T()))> x);
