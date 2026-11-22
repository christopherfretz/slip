#include "side.h"
// §6.1 FpOptions `arcp`.
#ifdef B
void f() {
#pragma clang fp reciprocal(on)
}
#else
void f() {
#pragma clang fp reciprocal(off)
}
#endif
