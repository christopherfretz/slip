#include "side.h"
// §5.4 FnSpec `0-defaulted`: defaulted on its first declaration against user-provided.
#ifdef B
struct C { C() = default; };
#else
struct C { C() {} };
#endif
