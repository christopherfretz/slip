#include "side.h"
// §7.3 CompoundAssignExtra `result_ty`.
short sh;
#ifdef B
int v = (sh *= 1);
#else
int v = (sh *= 1.0);
#endif
