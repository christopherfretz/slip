#include "side.h"
// WP-034 (§6): `FP_CONTRACT DEFAULT` clears the mask and restores `FpOverride::NONE`'s value.
#ifdef B
#pragma STDC FP_CONTRACT OFF
#pragma STDC FP_CONTRACT DEFAULT
void f() {}
#else
void f() {}
#endif
