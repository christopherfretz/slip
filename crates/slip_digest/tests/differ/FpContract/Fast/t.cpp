#include "side.h"
// §6.1 FpContract `Fast`.
#ifdef B
void f() {
#pragma clang fp contract(fast)
}
#else
void f() {
#pragma STDC FP_CONTRACT ON
}
#endif
