#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt64x3`.
#ifdef B
__clang_svuint8x3_t *v;
#else
__clang_svint64x3_t *v;
#endif
