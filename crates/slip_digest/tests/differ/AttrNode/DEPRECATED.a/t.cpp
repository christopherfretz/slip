#include "side.h"
// §10.1 AttrNode: `DEPRECATED.a`: a message against none.
#ifdef B
[[deprecated("m")]] void f();
#else
[[deprecated]] void f();
#endif
