#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt8x2`.
#ifdef B
__clang_svint16x2_t *v;
#else
__clang_svint8x2_t *v;
#endif
