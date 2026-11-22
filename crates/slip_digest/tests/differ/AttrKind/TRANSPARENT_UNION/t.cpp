#include "side.h"
// §10.1 AttrKind: `TRANSPARENT_UNION`: the attribute on one side only.
#ifdef B
union __attribute__((transparent_union)) U { int *a; };
#else
union U { int *a; };
#endif
