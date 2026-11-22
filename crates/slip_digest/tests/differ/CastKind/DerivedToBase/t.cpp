#include "side.h"
// §7.2 CastKind `DerivedToBase`.
struct Bs { int bm; };
struct Dd : Bs { int dm; };
Bs bs; Dd dd;
#ifdef B
Bs &v = bs;
#else
Bs &v = dd;
#endif
