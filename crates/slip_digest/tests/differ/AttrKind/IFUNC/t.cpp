#include "side.h"
// §10.1 AttrKind: `IFUNC`: the attribute on one side only.
extern "C" void (*r())() { return 0; }
#ifdef B
extern "C" void g() __attribute__((ifunc("r")));
#else
extern "C" void g();
#endif
