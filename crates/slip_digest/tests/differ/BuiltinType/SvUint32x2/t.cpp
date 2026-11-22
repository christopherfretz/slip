#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvUint32x2`.
#ifdef B
__clang_svuint64x2_t *v;
#else
__clang_svuint32x2_t *v;
#endif
