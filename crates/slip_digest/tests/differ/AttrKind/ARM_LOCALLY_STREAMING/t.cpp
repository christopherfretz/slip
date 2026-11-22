#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv9-a+sme
// §10.1 AttrKind `ARM_LOCALLY_STREAMING`: against `__arm_new`.
#ifdef B
__arm_new("za") void f() {}
#else
__arm_locally_streaming void f() {}
#endif
