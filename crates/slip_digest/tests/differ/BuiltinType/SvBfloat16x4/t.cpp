#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvBfloat16x4`.
#ifdef B
__clang_svboolx2_t *v;
#else
__clang_svbfloat16x4_t *v;
#endif
