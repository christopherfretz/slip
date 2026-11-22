#include "side.h"
// §7.2 CastKind `ConstructorConversion`.
struct Tc { Tc(int); };
#ifdef B
Tc v(1);
#else
Tc v = 1;
#endif
