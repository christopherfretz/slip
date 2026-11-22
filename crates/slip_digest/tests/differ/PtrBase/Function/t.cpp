#include "side.h"
// §8 PtrBase `Function` against `Decl`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&g>* x;
#else
A<&f1>* x;
#endif
