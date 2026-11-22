#include "side.h"
// §10.1 AttrNode: `ALIAS.a`: the fact differs.
extern "C" void h() {}
extern "C" void i() {}
#ifdef B
extern "C" void g() __attribute__((alias("i")));
#else
extern "C" void g() __attribute__((alias("h")));
#endif
