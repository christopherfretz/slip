// std: c++23
#include "side.h"
// §5.4 FnSpec `0-explicit-object`: the key changes too (§3).
#ifdef B
struct C { void f(this C&); };
#else
struct C { void f(); };
#endif
