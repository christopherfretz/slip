#include "side.h"
// target: aarch64-unknown-linux-gnu
// §10.1 AttrKind `TARGET_VERSION`: the default version (the only one Kiln declares without `fmv`) on one side only.
#ifdef B
int f();
#else
__attribute__((target_version("default"))) int f();
#endif
