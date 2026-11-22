#include "side.h"
// §5.5 CtorInitKind `Field`.
#ifdef B
struct C { int a; C() : a(1) {} };
#else
struct C { int a; C() {} };
#endif
