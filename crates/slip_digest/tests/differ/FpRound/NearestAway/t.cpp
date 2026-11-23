#include "side.h"
// §6.1 FpRound `NearestAway`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_TONEARESTFROMZERO
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
