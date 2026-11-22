#include "side.h"
// §10.1 AttrNode: `ABI_TAG.a`: the fact differs.
#ifdef B
__attribute__((abi_tag("v2"))) void f();
#else
__attribute__((abi_tag("v1"))) void f();
#endif
