#include "side.h"
// §8 ValueKind `Int`: an integer against a null pointer.
template <auto V> struct A;
#ifdef B
A<(int*)nullptr>* x;
#else
A<0>* x;
#endif
