#include "side.h"
// §10.1 AttrKind: `MINSIZE`: the attribute on one side only.
#ifdef B
__attribute__((minsize)) void f();
#else
void f();
#endif
