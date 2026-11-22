#include "side.h"
// §5.4 FnSpec `0-implicit-inline`: a member defined in its class.
#ifdef B
struct C { void f() {} };
#else
struct C { void f(); }; void C::f() {}
#endif
