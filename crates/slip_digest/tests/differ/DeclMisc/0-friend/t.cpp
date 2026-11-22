#include "side.h"
// §5.3 DeclMisc `0-friend`: a function only a friend declaration declares.
#ifdef B
struct C { friend void f(); };
#else
struct C { friend void f(); }; void f();
#endif
