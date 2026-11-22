#include "side.h"
// §8 Scalar `Pointer.ty`: `&g` as `int *` against `const int *`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<(const int*)&g>* x;
#else
A<&g>* x;
#endif
