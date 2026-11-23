#include "side.h"
// §6.1 FpRound `Nearest`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_TONEAREST
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
