#include "side.h"
// §6.1 FpRound `Dynamic`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_DYNAMIC
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
