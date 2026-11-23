#include "side.h"
// §6.1 FpOptions `round`.
#ifdef B
void f() {
#pragma STDC FENV_ROUND FE_DOWNWARD
}
#else
void f() {
#pragma STDC FENV_ROUND FE_UPWARD
}
#endif
