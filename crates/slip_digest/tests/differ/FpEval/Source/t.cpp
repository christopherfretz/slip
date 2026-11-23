#include "side.h"
// §6.1 FpEval `Source`.
#ifdef B
void f() {
#pragma clang fp eval_method(source)
}
#else
void f() {
#pragma clang fp eval_method(double)
}
#endif
