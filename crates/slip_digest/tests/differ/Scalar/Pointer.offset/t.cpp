#include "side.h"
// §8 Scalar `Pointer.offset`: `&arr[1]` against `&arr[2]`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&arr[2]>* x;
#else
A<&arr[1]>* x;
#endif
