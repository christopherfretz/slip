#include "side.h"
// §10.1 AttrNode: `LIFETIMEBOUND.a`: the fact differs.
#ifdef B
struct S { const int &f(const int &x) [[clang::lifetimebound]]; };
#else
struct S { const int &f(const int &x [[clang::lifetimebound]]); };
#endif
