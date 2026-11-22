#include "side.h"
// §7.2 CastKind `BaseToDerived`.
struct Bs { int bm; };
struct Dd : Bs { int dm; };
Bs bs; Dd dd;
#ifdef B
Dd &v = dd;
#else
Dd &v = static_cast<Dd&>(bs);
#endif
