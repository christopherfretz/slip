#include "side.h"
// §6.1 FpExcept `MayTrap`.
#ifdef B
void f() {
#pragma clang fp exceptions(maytrap)
}
#else
void f() {
#pragma clang fp exceptions(strict)
}
#endif
