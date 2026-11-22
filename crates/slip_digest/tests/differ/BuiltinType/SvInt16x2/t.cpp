#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt16x2`.
#ifdef B
__clang_svint32x2_t *v;
#else
__clang_svint16x2_t *v;
#endif
