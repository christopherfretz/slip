// std: c++26
#include "side.h"
// digest-kinds rule §3.3: two classes declared by name-independent members `_`, their bodies swapped.
struct S {
#ifdef B
struct { int b; } _; struct { int a; } _;
#else
struct { int a; } _; struct { int b; } _;
#endif
};
int use = sizeof(S);
