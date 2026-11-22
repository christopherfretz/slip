#include "side.h"
// §10.1 AttrKind: `RETAIN`: the attribute on one side only.
#ifdef B
__attribute__((retain,used)) static void f() {};
#else
static void f() {};
#endif
