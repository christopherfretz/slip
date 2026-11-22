#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvFloat64x3`.
#ifdef B
__clang_svbfloat16x3_t *v;
#else
__clang_svfloat64x3_t *v;
#endif
