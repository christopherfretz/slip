#include "side.h"
// §6.1 FpOptions `except`.
#ifdef B
void f() {
#pragma clang fp exceptions(strict)
}
#else
void f() {
#pragma clang fp exceptions(ignore)
}
#endif
