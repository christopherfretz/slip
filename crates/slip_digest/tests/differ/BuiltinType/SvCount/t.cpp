#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvCount`.
#ifdef B
__clang_svint8x2_t *v;
#else
__SVCount_t *v;
#endif
