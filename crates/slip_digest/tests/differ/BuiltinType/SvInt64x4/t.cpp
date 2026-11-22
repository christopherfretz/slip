#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt64x4`.
#ifdef B
__clang_svuint8x4_t *v;
#else
__clang_svint64x4_t *v;
#endif
