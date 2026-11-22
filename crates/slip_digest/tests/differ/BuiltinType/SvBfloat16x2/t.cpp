#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvBfloat16x2`.
#ifdef B
__clang_svint8x3_t *v;
#else
__clang_svbfloat16x2_t *v;
#endif
