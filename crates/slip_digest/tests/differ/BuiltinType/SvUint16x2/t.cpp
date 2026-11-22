#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvUint16x2`.
#ifdef B
__clang_svuint32x2_t *v;
#else
__clang_svuint16x2_t *v;
#endif
