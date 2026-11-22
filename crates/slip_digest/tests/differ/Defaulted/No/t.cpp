#include "side.h"
// §5.4 Defaulted `No`.
#ifdef B
struct C { C(); };
#else
struct C { C() = default; };
#endif
