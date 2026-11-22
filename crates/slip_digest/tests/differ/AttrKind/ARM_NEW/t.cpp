#include "side.h"
// target: aarch64-unknown-linux-gnu
// args: -march=armv9-a+sme
// §10.1 AttrKind `ARM_NEW`: against `__arm_locally_streaming`, the
// keyword-syntax function attribute nearest in shape. The sides also
// differ by `AttrNode/ARM_NEW.a` (`UvIn`): no other function attribute
// writes one such operand alone (`PASS_OBJECT_SIZE.a` is a GNU parameter
// attribute with a `.b`), so `ARM_NEW` written as `ARM_LOCALLY_STREAMING`
// is killed here only together with `ARM_NEW.a` skipped.
#ifdef B
__arm_locally_streaming void f() {}
#else
__arm_new("za") void f() {}
#endif
