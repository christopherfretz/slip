#include "side.h"
// §10.1 AttrNode: `LIFETIMEBOUND.flags-args`: only the spelling differs.
#ifdef B
const int &f(const int &x [[clang::lifetimebound()]]);
#else
const int &f(const int &x [[clang::lifetimebound]]);
#endif
