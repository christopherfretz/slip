#include "side.h"
// §11.9: a constructor defaulted on its first declaration is defined when odr-used (WP-026).
struct M { M(); };
struct S { M m; S() = default; };
#ifdef B
void g() { S s; }
#else

#endif
