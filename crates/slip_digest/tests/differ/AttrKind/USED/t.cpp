#include "side.h"
// §10.1 AttrKind: `USED`: the attribute on one side only.
#ifdef B
__attribute__((used)) static void f() {};
#else
static void f() {};
#endif
