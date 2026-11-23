// std: c++17
#include "side.h"
// §4.3 AutoKeyword `DecltypeAuto`.
#ifdef B
decltype(auto) f();
#else
auto f();
#endif
