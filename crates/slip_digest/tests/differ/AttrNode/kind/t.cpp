#include "side.h"
// §10.1 AttrNode: `kind`: one kind for another.
#ifdef B
__attribute__((hot)) void f();
#else
__attribute__((cold)) void f();
#endif
