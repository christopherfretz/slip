#include "side.h"
// §6.1 FpOverride `strict_fn`: the inner block is constrained, which makes the body strict: the inner block's override differs too.
#ifdef B
void f() { {
#pragma STDC FENV_ACCESS ON
} }
#else
void f() { {
} }
#endif
