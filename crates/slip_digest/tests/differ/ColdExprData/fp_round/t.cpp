#include "side.h"
// §7.4 ColdExprData `fp_round`.
double d;
#ifdef B
double v = d + d;
#else
#pragma STDC FENV_ROUND FE_UPWARD
double v = d + d;
#endif
