#include "side.h"
// §6.1 FpEval `Double`.
#ifdef B
void f() {
#pragma clang fp eval_method(double)
}
#else
void f() {
#pragma clang fp eval_method(source)
}
#endif
