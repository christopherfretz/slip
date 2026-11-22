// std: c++23
#include "side.h"
// §10.1 AttrKind: `ASSUME`: the attribute on one side only.
#ifdef B
void f(int x) { [[assume(x > 0)]]; }
#else
void f(int x) { }
#endif
