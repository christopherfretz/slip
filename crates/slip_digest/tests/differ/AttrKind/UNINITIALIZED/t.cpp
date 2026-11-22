#include "side.h"
// §10.1 AttrKind: `UNINITIALIZED`: the attribute on one side only.
#ifdef B
void f() { int x [[clang::uninitialized]]; }
#else
void f() { int x; }
#endif
