#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `SvBoolx4`.
#ifdef B
__SVMfloat8_t *v;
#else
__clang_svboolx4_t *v;
#endif
