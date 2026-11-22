#include "side.h"
// §5.5 CtorInitKind `Delegating`.
#ifdef B
struct C { C(int); C() : C(1) {} };
#else
struct C { C(int); C() {} };
#endif
