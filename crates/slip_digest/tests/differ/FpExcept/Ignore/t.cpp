#include "side.h"
// §6.1 FpExcept `Ignore`.
#ifdef B
void f() {
#pragma clang fp exceptions(ignore)
}
#else
void f() {
#pragma clang fp exceptions(strict)
}
#endif
