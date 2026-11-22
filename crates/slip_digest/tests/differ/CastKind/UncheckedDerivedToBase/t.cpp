#include "side.h"
// §7.2 CastKind `UncheckedDerivedToBase`.
struct Bs { int bm; };
struct Dd : Bs { int dm; };
Bs bs; Dd dd;
#ifdef B
int v = bs.bm;
#else
int v = dd.bm;
#endif
