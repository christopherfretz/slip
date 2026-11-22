#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvUint16x3`.
#ifdef B
__clang_svuint32x3_t *v;
#else
__clang_svuint16x3_t *v;
#endif
