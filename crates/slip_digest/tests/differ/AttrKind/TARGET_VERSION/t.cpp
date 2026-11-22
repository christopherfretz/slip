#include "side.h"
// target: aarch64-unknown-linux-gnu
// §10.1 AttrKind `TARGET_VERSION`: against `alias`, whose operand is the same string (`String(A)`). `default` is
// the only version Kiln declares without `fmv`; `g` is the alias's target.
#ifdef B
int g() asm("default");
int g() { return 0; }
__attribute__((alias("default"))) int f();
#else
int g() asm("default");
int g() { return 0; }
__attribute__((target_version("default"))) int f();
#endif
