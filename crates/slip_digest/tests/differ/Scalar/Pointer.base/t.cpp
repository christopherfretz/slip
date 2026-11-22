#include "side.h"
// §8 Scalar `Pointer.base`: `&g` against `&h`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&h>* x;
#else
A<&g>* x;
#endif
