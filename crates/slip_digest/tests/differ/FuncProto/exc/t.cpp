#include "side.h"
// §4.5 FuncProto `exc` (§4.2 rule 3: in the type from C++17).
#ifdef B
void (*v)() noexcept;
#else
void (*v)();
#endif
