#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvUint64x3`.
#ifdef B
__clang_svfloat16x3_t *v;
#else
__clang_svuint64x3_t *v;
#endif
