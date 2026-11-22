#include "side.h"
// §5.4 FnSpec `0-virtual`.
#ifdef B
struct C { virtual void f(); };
#else
struct C { void f(); };
#endif
