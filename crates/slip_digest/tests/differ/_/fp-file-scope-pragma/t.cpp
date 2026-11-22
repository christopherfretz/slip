#include "side.h"
// WP-034 (§6): a pragma at file scope reaches the body: the block records the state in effect.
#ifdef B
#pragma STDC FP_CONTRACT OFF
void f() {}
#else
void f() {}
#endif
