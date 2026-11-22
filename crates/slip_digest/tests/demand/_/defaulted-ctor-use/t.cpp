#include "side.h"
// §11.9: a constructor defaulted on its first declaration is defined when odr-used.
struct S { S() = default; };
#ifdef B
void g() { S s; }
#else

#endif
