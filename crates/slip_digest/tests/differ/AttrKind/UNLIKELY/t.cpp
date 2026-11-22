#include "side.h"
// §10.1 AttrKind: `UNLIKELY`: the attribute on one side only.
#ifdef B
void f(int x) { if (x) [[unlikely]] { } }
#else
void f(int x) { if (x) { } }
#endif
