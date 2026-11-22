#include "side.h"
// target: aarch64-unknown-linux-gnu
// §10.1 AttrNode `TARGET_VERSION.a`: the string differs (Kiln trims it to `default`: the only version it declares without `fmv`).
#ifdef B
__attribute__((target_version(" default "))) int f();
#else
__attribute__((target_version("default"))) int f();
#endif
