#include "side.h"
// §5.5 CtorInit `Base.target`: the initialisers of two bases exchanged.
#ifdef B
struct P { P(int); }; struct Q { Q(int); }; struct C : P, Q { C() : P(2), Q(1) {} };
#else
struct P { P(int); }; struct Q { Q(int); }; struct C : P, Q { C() : P(1), Q(2) {} };
#endif
