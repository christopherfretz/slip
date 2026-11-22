#include "side.h"
// §10.1 AttrKind: `XRAY_LOG_ARGS`: the attribute on one side only.
#ifdef B
__attribute__((xray_always_instrument, xray_log_args(1))) void f(int);
#else
void f(int);
#endif
