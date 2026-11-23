#include "side.h"
// §6.1 FpRound `Upward`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#else
void f() {
#pragma STDC FENV_ROUND FE_DOWNWARD
}
#endif
