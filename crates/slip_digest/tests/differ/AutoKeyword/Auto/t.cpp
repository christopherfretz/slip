// std: c++17
#include "side.h"
// §4.3 AutoKeyword `Auto`: `auto` against `decltype(auto)`, undeduced.
#ifdef B
auto f();
#else
decltype(auto) f();
#endif
