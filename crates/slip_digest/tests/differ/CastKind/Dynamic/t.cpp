#include "side.h"
// §7.2 CastKind `Dynamic`.
struct P { virtual ~P(); };
struct Q : P {};
P *pp;
#ifdef B
Q *v = static_cast<Q*>(pp);
#else
Q *v = dynamic_cast<Q*>(pp);
#endif
