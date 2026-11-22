#include "side.h"
// §7.3 CompoundAssignExtra `lhs_ty`.
short sh;
#ifdef B
int v = (sh += 1);
#else
int v = (sh += 1L);
#endif
