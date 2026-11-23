#include "side.h"
// §6.1 FpRound `Downward`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_DOWNWARD
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
