#include "side.h"
// §6.1 FpOverride `vals`.
#ifdef B
void f() {
#pragma STDC FP_CONTRACT OFF
}
#else
void f() {
#pragma STDC FP_CONTRACT ON
}
#endif
