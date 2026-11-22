#include "side.h"
// §4.5 FuncInfo `0-trailing-return` (§4.2 rule 6).
#ifdef B
auto (*v)() -> int;
#else
int (*v)();
#endif
