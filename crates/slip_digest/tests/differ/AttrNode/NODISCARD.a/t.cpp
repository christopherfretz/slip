#include "side.h"
// §10.1 AttrNode: `NODISCARD.a`: the fact differs.
#ifdef B
[[nodiscard("m")]] int f();
#else
[[nodiscard]] int f();
#endif
