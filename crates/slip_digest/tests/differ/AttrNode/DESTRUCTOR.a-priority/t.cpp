#include "side.h"
// §10.1 AttrNode: `DESTRUCTOR.a-priority`: the fact differs.
#ifdef B
__attribute__((destructor(102))) void f();
#else
__attribute__((destructor(101))) void f();
#endif
