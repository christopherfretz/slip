#include "side.h"
// §4.4 "Spellings": a found set in a dependent array bound of a parameter type.
int f(int);
template<class T> void g(T t, char (*p)[sizeof(f(T()))]);
#ifdef B
long f(long);
#endif
template<class T> void h(T t, char (*p)[sizeof(f(T()))]);
