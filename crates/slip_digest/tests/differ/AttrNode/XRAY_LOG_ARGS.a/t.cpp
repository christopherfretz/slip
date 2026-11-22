#include "side.h"
// §10.1 AttrNode: `XRAY_LOG_ARGS.a`: the fact differs.
#ifdef B
__attribute__((xray_always_instrument, xray_log_args(2))) void f(int, int);
#else
__attribute__((xray_always_instrument, xray_log_args(1))) void f(int, int);
#endif
