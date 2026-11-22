#include "side.h"
// §10.1 AttrKind: `TYPE_VISIBILITY`: the attribute on one side only.
#ifdef B
struct __attribute__((type_visibility("hidden"))) S {};
#else
struct S {};
#endif
