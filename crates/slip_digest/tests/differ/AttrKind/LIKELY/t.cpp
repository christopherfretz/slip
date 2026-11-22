#include "side.h"
// §10.1 AttrKind: `LIKELY`: the attribute on one side only.
#ifdef B
void f(int x) { if (x) [[likely]] { } }
#else
void f(int x) { if (x) { } }
#endif
