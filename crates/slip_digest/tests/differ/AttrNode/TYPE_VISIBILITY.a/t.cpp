#include "side.h"
// §10.1 AttrNode: `TYPE_VISIBILITY.a`: the fact differs.
#ifdef B
struct __attribute__((type_visibility("protected"))) S {};
#else
struct __attribute__((type_visibility("hidden"))) S {};
#endif
