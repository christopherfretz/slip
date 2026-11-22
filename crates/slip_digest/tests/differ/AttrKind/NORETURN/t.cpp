#include "side.h"
// §10.1 AttrKind: `NORETURN`: the attribute on one side only.
#ifdef B
[[noreturn]] void f();
#else
void f();
#endif
