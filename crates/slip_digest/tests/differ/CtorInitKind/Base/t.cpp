#include "side.h"
// §5.5 CtorInitKind `Base`.
#ifdef B
struct P { P(int); }; struct C : P { C() : P(1) {} };
#else
struct P { P(int); }; struct C : virtual P { C() : P(1) {} };
#endif
