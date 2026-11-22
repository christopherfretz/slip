#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv9-a+sme2
// §10.1 AttrNode `ARM_NEW.a`: the states differ.
#ifdef B
__arm_new("za", "zt0") void f() {}
#else
__arm_new("za") void f() {}
#endif
