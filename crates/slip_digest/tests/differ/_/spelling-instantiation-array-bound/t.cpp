#include "side.h"
// §4.4 "Spellings", rule 7: an instantiation-dependent bound of an array whose size is known.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, char (*)[sizeof(sizeof(f(T())))]);
