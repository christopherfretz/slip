#include "side.h"
// §6.1 FpOptions `eval`.
#ifdef B
void f() {
#pragma clang fp eval_method(extended)
}
#else
void f() {
#pragma clang fp eval_method(source)
}
#endif
