#include "side.h"
// §5.5 CtorInit `VirtualBase.target`: the initialisers of two virtual bases exchanged.
#ifdef B
struct P { P(int); }; struct Q { Q(int); }; struct C : virtual P, virtual Q { C() : P(2), Q(1) {} };
#else
struct P { P(int); }; struct Q { Q(int); }; struct C : virtual P, virtual Q { C() : P(1), Q(2) {} };
#endif
