#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvMfloat8x4`.
#ifdef B
__mfp8 *v;
#else
__clang_svmfloat8x4_t *v;
#endif
