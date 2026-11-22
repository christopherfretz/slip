#include "side.h"
// §4.5 CallConv `C` (row test left: on this target `sysv_abi` is the default) is `Default` (§4.2 rule 5).
#ifdef B
void (__attribute__((sysv_abi)) *v)();
#else
void (*v)();
#endif
