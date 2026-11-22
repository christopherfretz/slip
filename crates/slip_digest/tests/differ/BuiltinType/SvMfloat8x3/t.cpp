#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvMfloat8x3`.
#ifdef B
__clang_svmfloat8x4_t *v;
#else
__clang_svmfloat8x3_t *v;
#endif
