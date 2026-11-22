// std: c++23
#include "side.h"
// §5.4 ParamData `flags-explicit-object`: the key changes too; both keys print alike.
#ifdef B
struct C { void f(this C& self); };
#else
struct C { void f(C& self); };
#endif
