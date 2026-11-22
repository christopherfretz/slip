#include "side.h"
// §8 ValueKind `Aggregate`: a class value against an integer.
template <auto V> struct A;
struct S { int i; };
#ifdef B
A<0>* x;
#else
A<S{0}>* x;
#endif
