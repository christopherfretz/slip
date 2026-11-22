#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv9-a+sme
// §10.1 AttrKind `ARM_LOCALLY_STREAMING`: against C11 `_Noreturn`
// (`NORETURN`), the other keyword-syntax function attribute with no
// operand and no flag, so only the kind tells the sides apart.
#ifdef B
_Noreturn void f() { for (;;) {} }
#else
__arm_locally_streaming void f() { for (;;) {} }
#endif
