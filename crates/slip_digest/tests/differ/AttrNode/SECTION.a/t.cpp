#include "side.h"
// §10.1 AttrNode: `SECTION.a`: the fact differs.
#ifdef B
int v __attribute__((section("t")));
#else
int v __attribute__((section("s")));
#endif
