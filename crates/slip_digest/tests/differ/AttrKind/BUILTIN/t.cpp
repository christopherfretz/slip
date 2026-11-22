#include "side.h"
// §10.1 AttrKind: `BUILTIN`: a lazily declared builtin carries the implicit attribute.
#ifdef B
void f() { __builtin_abs(1); }
#else
void f() { }
#endif
