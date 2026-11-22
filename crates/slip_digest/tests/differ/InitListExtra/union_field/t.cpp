#include "side.h"
// WP-035, digest-kinds §7 `InitListExtra` `union_field`: outside a template the designator is not kept: the member is.
union U { int i; int j; };
#ifdef B
U v = {.j = 1};
#else
U v = {.i = 1};
#endif
