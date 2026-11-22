#include "side.h"
// §5.7 ColdDeclData `overrides`: a member that overrides the functions of two bases against one that overrides one of them.
struct A { virtual void f(); };
#ifdef B
struct C { virtual void f(int); };
#else
struct C { virtual void f(); };
#endif
struct D : A, C { void f(); };
