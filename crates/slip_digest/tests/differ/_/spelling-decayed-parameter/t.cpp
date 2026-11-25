#include "side.h"
// §4.4 "Spellings", rule 2, through `TypeView` `Decayed` (Kiln WP-2720): a parameter of a function type, written as an array whose bound is instantiation-dependent, is adjusted to a pointer that keeps the declared array type; its bound's found set is read from it.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, void (*p)(char [sizeof(sizeof(f(T())))]));
