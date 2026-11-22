// std: c++17
#include "side.h"
// §5.5 DefinitionBits `0-defaulted-default-ctor-constexpr`.
#ifdef B
struct C { int a = 0; };
#else
struct C { int a; };
#endif
