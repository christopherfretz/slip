#include "side.h"
// §7.2 CastKind `DerivedToBaseMemberPointer`.
struct Bs { int bm; };
struct Dd : Bs { int dm; };
Bs bs; Dd dd;
#ifdef B
int Bs::*v = &Bs::bm;
#else
int Bs::*v = static_cast<int Bs::*>(&Dd::dm);
#endif
