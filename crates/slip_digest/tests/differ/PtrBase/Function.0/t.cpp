#include "side.h"
// §8 PtrBase `Function.0`: `&f1` against `&f2`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&f2>* x;
#else
A<&f1>* x;
#endif
