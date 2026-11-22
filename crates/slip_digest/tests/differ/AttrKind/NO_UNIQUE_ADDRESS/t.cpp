#include "side.h"
// §10.1 AttrKind: `NO_UNIQUE_ADDRESS`: the attribute on one side only.
#ifdef B
struct E {}; struct S { [[no_unique_address]] E e; };
#else
struct E {}; struct S { E e; };
#endif
