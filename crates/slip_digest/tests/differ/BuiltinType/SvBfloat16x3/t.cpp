#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvBfloat16x3`.
#ifdef B
__clang_svint8x4_t *v;
#else
__clang_svbfloat16x3_t *v;
#endif
