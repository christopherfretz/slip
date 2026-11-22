#include "side.h"
// target: aarch64-unknown-linux-gnu
// §10.1 AttrKind `BUILTIN_ALIAS`: against `cold`.
#ifdef B
__attribute__((cold)) __SVInt8_t f(__SVBool_t, __SVInt8_t, __SVInt8_t);
#else
__attribute__((__clang_arm_builtin_alias(__builtin_sve_svabd_s8_m))) __SVInt8_t f(__SVBool_t, __SVInt8_t, __SVInt8_t);
#endif
