#include "side.h"
// §6.1 FpOptions `fenv_access`.
#ifdef B
void f() {
#pragma STDC FENV_ACCESS ON
}
#else
void f() {
#pragma STDC FENV_ACCESS OFF
}
#endif
