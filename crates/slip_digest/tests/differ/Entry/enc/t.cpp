#include "side.h"
// §10.3 Entry `enc`: the same code unit as `char8_t` and as `char` (`"a"` against `u8"a"`).
#ifdef B
constexpr const void* p = u8"a";
#else
constexpr const void* p = "a";
#endif
