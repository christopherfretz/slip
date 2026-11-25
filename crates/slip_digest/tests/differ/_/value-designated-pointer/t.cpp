#include "side.h"
// digest §7: `&s.a` written as `&s.a` and as `&(&s)->a`.
// WP-156: the values are equal (x's type is equal), but the argument is an operand (site 4) that names something, written as an expression in x's operand unit (digest §2.4), and the two expressions differ: a derived change, loud.
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
