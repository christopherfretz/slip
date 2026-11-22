#include "side.h"
// §10.1 AttrNode: `IFUNC.a`: the fact differs.
extern "C" void (*r())() { return 0; }
extern "C" void (*q())() { return 0; }
#ifdef B
extern "C" void g() __attribute__((ifunc("q")));
#else
extern "C" void g() __attribute__((ifunc("r")));
#endif
