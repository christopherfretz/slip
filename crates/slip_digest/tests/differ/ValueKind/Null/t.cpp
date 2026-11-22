#include "side.h"
// §8 ValueKind `Null`: a null `int *` against `&g` (same type).
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&g>* x;
#else
A<(int*)nullptr>* x;
#endif
