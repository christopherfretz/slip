#include "side.h"
// §5.7 ColdDeclData `init_fp`: the floating-point state a default member initialiser was parsed in.
#ifdef B
#pragma STDC FP_CONTRACT OFF
#endif
struct S { double d = 1.0 * 2.0; };
