#include "side.h"
// target: aarch64-unknown-linux-gnu
// §4.3 BuiltinType `MFloat8`.
#ifdef B
__SVInt8_t *v;
#else
__mfp8 *v;
#endif
