#include "side.h"
// §6.1 FpContract `On`.
#ifdef B
void f() {
#pragma STDC FP_CONTRACT ON
}
#else
void f() {
#pragma STDC FP_CONTRACT OFF
}
#endif
