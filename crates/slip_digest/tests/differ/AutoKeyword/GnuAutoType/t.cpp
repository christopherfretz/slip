// std: c++17
#include "side.h"
// §4.3 AutoKeyword `GnuAutoType`: `__auto_type` against `auto`.
#ifdef B
__auto_type f();
#else
auto f();
#endif
