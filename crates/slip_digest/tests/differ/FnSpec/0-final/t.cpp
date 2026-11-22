#include "side.h"
// §5.4 FnSpec `0-final`.
#ifdef B
struct C { virtual void f(); }; struct D : C { void f() final; };
#else
struct C { virtual void f(); }; struct D : C { void f(); };
#endif
