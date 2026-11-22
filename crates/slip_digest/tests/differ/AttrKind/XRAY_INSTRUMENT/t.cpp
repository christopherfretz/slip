#include "side.h"
// §10.1 AttrKind: `XRAY_INSTRUMENT`: the attribute on one side only.
#ifdef B
__attribute__((xray_always_instrument)) void f();
#else
void f();
#endif
