#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvInt32x3`.
#ifdef B
__clang_svint64x3_t *v;
#else
__clang_svint32x3_t *v;
#endif
