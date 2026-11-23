#include "side.h"
// §6.1 FpOptions `math_errno`: `float_control(precise)` sets nnan, ninf, nsz, afn and math_errno together: no pragma sets one alone (a unit test of fp.rs holds each row).
#ifdef B
void f() {
#pragma float_control(precise, off)
}
#else
void f() {
#pragma float_control(precise, on)
}
#endif
