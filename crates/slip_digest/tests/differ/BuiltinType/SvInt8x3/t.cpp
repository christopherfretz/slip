#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt8x3`.
#ifdef B
__clang_svint16x3_t *v;
#else
__clang_svint8x3_t *v;
#endif
