#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvBoolx2`.
#ifdef B
__clang_svboolx4_t *v;
#else
__clang_svboolx2_t *v;
#endif
