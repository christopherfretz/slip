#include "side.h"
// §10.1 AttrKind: `ABI_TAG`: the attribute on one side only.
#ifdef B
__attribute__((abi_tag("v1"))) void f();
#else
void f();
#endif
