#include "side.h"
// digest §4.3: a local class with a nested class, renamed, unobserved.
#ifdef B
int f() { struct Y { struct I { int m; }; }; Y::I i{1}; return i.m; }
#else
int f() { struct X { struct I { int m; }; }; X::I i{1}; return i.m; }
#endif
