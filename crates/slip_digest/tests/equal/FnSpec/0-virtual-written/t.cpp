#include "side.h"
// §5.4 FnSpec `0-virtual-written`: an overrider is virtual whether or not `virtual` is written.
#ifdef B
struct C { virtual void f(); }; struct D : C { virtual void f(); };
#else
struct C { virtual void f(); }; struct D : C { void f(); };
#endif
