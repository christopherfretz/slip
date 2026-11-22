#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv9-a+sme
// §10.1 AttrKind `ARM_NEW`: against `__arm_locally_streaming`.
#ifdef B
__arm_locally_streaming void f() {}
#else
__arm_new("za") void f() {}
#endif
