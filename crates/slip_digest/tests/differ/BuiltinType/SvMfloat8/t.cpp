#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvMfloat8`.
#ifdef B
__clang_svmfloat8x2_t *v;
#else
__SVMfloat8_t *v;
#endif
