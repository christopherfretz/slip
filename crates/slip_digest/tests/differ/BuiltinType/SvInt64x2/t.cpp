#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt64x2`.
#ifdef B
__clang_svuint8x2_t *v;
#else
__clang_svint64x2_t *v;
#endif
