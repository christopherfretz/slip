#include "side.h"
// §8 ValueKind `Pointer`: `&f1` against a null pointer to function (same type).
template <auto V> struct A;
int g, h;
int arr[4];
void f1();
void f2();
struct S2 { int a; int b; } s;
#ifdef B
A<(void (*)())nullptr>* x;
#else
A<&f1>* x;
#endif
