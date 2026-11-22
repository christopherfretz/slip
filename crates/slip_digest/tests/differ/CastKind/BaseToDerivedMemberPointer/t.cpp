#include "side.h"
// §7.2 CastKind `BaseToDerivedMemberPointer`.
struct Bs { int bm; };
struct Dd : Bs { int dm; };
Bs bs; Dd dd;
#ifdef B
int Dd::*v = &Dd::dm;
#else
int Dd::*v = &Bs::bm;
#endif
