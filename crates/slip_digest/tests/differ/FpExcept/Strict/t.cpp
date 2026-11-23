#include "side.h"
// §6.1 FpExcept `Strict`.
#ifdef B
void f() {
#pragma clang fp exceptions(strict)
}
#else
void f() {
#pragma clang fp exceptions(ignore)
}
#endif
