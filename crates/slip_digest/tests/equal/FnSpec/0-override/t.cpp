#include "side.h"
// §5.4 FnSpec `0-override`: `override` is a spelling; the fact is `overrides`.
#ifdef B
struct C { virtual void f(); }; struct D : C { void f() override; };
#else
struct C { virtual void f(); }; struct D : C { void f(); };
#endif
