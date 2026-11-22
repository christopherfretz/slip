#include "side.h"
// §10.1 AttrNode: `XRAY_INSTRUMENT.a`: the fact differs.
#ifdef B
__attribute__((xray_never_instrument)) void f();
#else
__attribute__((xray_always_instrument)) void f();
#endif
