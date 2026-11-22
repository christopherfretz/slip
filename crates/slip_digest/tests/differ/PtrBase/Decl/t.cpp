#include "side.h"
// §8 PtrBase `Decl` against `Function`.
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<&f1>* x;
#else
A<&g>* x;
#endif
