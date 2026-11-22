#include "side.h"
// digest §7: `&s.a` written as `&s.a` and as `&(&s)->a`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&(&s)->a>* x;
#else
A<&s.a>* x;
#endif
