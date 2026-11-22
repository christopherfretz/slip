#include "side.h"
// §10.1 AttrKind: `RETURNS_TWICE`: the attribute on one side only.
#ifdef B
__attribute__((returns_twice)) void f();
#else
void f();
#endif
