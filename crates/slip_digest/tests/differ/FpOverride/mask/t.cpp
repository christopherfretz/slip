#include "side.h"
// §6.1 FpOverride `mask`: the pragma sets the value `FpOverride::NONE` has: only the mask differs.
#ifdef B
void f() {
#pragma STDC FP_CONTRACT ON
}
#else
void f() {
}
#endif
