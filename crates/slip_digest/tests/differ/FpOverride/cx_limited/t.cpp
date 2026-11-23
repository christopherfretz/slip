#include "side.h"
// §6.1 FpOverride `cx_limited`.
#ifdef B
void f() {
#pragma STDC CX_LIMITED_RANGE ON
}
#else
void f() {
#pragma STDC CX_LIMITED_RANGE OFF
}
#endif
