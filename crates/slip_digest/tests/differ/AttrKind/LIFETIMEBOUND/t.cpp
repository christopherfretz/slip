#include "side.h"
// §10.1 AttrKind: `LIFETIMEBOUND`: the attribute on one side only.
#ifdef B
const int &f(const int &x [[clang::lifetimebound]]);
#else
const int &f(const int &x);
#endif
