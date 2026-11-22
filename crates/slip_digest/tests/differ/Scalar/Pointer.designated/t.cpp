#include "side.h"
// §8 Scalar `Pointer.designated`: `void *` to `s` against `void *` to `s.a` (same address).
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<static_cast<void*>(&s.a)>* x;
#else
A<static_cast<void*>(&s)>* x;
#endif
