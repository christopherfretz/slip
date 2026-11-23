#include "side.h"
// §6.1 FpRound `TowardZero`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_TOWARDZERO
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
