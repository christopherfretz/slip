#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvFloat32x4`.
#ifdef B
__clang_svfloat64x4_t *v;
#else
__clang_svfloat32x4_t *v;
#endif
