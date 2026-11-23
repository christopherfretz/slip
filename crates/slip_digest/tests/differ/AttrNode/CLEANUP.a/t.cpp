#include "side.h"
// §10.1 AttrNode `CLEANUP.a`: the function `cleanup` names. The attribute
// is on a local variable, so the test needs function bodies.
void c1(int *);
void c2(int *);
#ifdef B
void f() { int v __attribute__((cleanup(c2))) = 0; }
#else
void f() { int v __attribute__((cleanup(c1))) = 0; }
#endif
