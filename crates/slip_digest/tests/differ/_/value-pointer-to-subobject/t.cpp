#include "side.h"
// §8 Scalar `Pointer.offset`, digest §7: pointers to two members of one object.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&s.b>* x;
#else
A<&s.a>* x;
#endif
