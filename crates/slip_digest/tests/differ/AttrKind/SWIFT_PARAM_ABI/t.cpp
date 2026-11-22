#include "side.h"
// §10.1 AttrKind: `SWIFT_PARAM_ABI`: the attribute on one side only.
#ifdef B
void f(void *p __attribute__((swift_context))) __attribute__((swiftcall));
#else
void f(void *p) __attribute__((swiftcall));
#endif
