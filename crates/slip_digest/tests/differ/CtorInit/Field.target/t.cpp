#include "side.h"
// §5.5 CtorInit `Field.target`.
#ifdef B
struct C { int a, b; C() : b(1) {} };
#else
struct C { int a, b; C() : a(1) {} };
#endif
