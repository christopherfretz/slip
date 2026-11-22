#include "side.h"
// §10.1 AttrNode: `SWIFT_PARAM_ABI.a`: the fact differs.
#ifdef B
void f(void *p __attribute__((swift_async_context))) __attribute__((swiftcall));
#else
void f(void *p __attribute__((swift_context))) __attribute__((swiftcall));
#endif
