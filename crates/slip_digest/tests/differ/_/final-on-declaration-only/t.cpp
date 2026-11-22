#include "side.h"
// §2.3, §5.4 FnSpec `0-final`: written on the in-class declaration only; the walk reads the definition.
#ifdef B
struct C { virtual void f(); };
struct D : C { void f() final; };
void D::f() {}
#else
struct C { virtual void f(); };
struct D : C { void f(); };
void D::f() {}
#endif
