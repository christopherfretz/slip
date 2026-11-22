#include "side.h"
// §10.1 AttrArena: `decl_attrs`: a declaration's attributes are in its unit.
#ifdef B
__attribute__((cold)) void f();
#else
void f();
#endif
