#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvFloat16x2`.
#ifdef B
__clang_svfloat32x2_t *v;
#else
__clang_svfloat16x2_t *v;
#endif
