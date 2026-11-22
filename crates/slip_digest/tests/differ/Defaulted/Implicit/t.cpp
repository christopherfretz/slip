#include "side.h"
// §5.4 Defaulted `Implicit`: an implicitly declared default constructor against one defaulted on its declaration; the implicit one is a derived unit (§11.9).
#ifdef B
struct C {};
C c;
#else
struct C { C() = default; };
C c;
#endif
