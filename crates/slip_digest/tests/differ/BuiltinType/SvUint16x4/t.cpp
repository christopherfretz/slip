#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvUint16x4`.
#ifdef B
__clang_svuint32x4_t *v;
#else
__clang_svuint16x4_t *v;
#endif
