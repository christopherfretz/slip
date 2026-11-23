#include "side.h"
// §6.1 FpOptions `reassoc`.
#ifdef B
void f() {
#pragma clang fp reassociate(on)
}
#else
void f() {
#pragma clang fp reassociate(off)
}
#endif
