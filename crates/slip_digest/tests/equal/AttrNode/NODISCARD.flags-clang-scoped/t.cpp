#include "side.h"
// §10.1 AttrNode: `NODISCARD.flags-clang-scoped`: only the spelling differs.
#ifdef B
[[nodiscard]] int f();
#else
[[clang::warn_unused_result]] int f();
#endif
