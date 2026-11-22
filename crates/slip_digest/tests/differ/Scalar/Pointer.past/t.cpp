#include "side.h"
// §8 Scalar `Pointer.past`: `&s.a + 1` (one past `s.a`) against `&s.b` (same address).
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&s.b>* x;
#else
A<&s.a + 1>* x;
#endif
