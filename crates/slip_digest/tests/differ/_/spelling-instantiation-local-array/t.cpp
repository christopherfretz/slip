#include "side.h"
// §4.4 "Spellings", rule 7: an instantiation-dependent bound of a local array.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t) { char a[sizeof(sizeof(f(t)))]; }
