#include "side.h"
// §10.1 AttrKind: `ALIAS`: the attribute on one side only.
extern "C" void h() {}
#ifdef B
extern "C" void g() __attribute__((alias("h")));
#else
extern "C" void g();
#endif
