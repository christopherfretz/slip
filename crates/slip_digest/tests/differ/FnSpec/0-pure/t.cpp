#include "side.h"
// §5.4 FnSpec `0-pure`.
#ifdef B
struct C { virtual void f() = 0; };
#else
struct C { virtual void f(); };
#endif
