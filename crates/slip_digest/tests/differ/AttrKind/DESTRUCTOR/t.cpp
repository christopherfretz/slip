#include "side.h"
// §10.1 AttrKind: `DESTRUCTOR`: the attribute on one side only.
#ifdef B
__attribute__((destructor)) void f();
#else
void f();
#endif
