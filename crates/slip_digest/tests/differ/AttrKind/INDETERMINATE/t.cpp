// std: c++26
#include "side.h"
// §10.1 AttrKind: `INDETERMINATE`: the attribute on one side only.
#ifdef B
void f() { int x [[indeterminate]]; }
#else
void f() { int x; }
#endif
