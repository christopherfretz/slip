#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt8x4`.
#ifdef B
__clang_svint16x4_t *v;
#else
__clang_svint8x4_t *v;
#endif
