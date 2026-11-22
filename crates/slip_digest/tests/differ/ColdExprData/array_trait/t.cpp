#include "side.h"
// §7.4 ColdExprData: written by `expr_misc::array_trait_extra`
#ifdef B
unsigned long v = __array_rank(int[1]);
#else
unsigned long v = __array_rank(int[1][2]);
#endif
