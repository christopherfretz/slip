#include "side.h"
// §5.5 CtorInit `init`.
#ifdef B
struct C { int a; C() : a(2) {} };
#else
struct C { int a; C() : a(1) {} };
#endif
