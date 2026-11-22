#include "side.h"
// §10.3 StringEncoding `Utf8`: `u8"a"` against `"a"` (reached only through a variable's value).
#ifdef B
constexpr auto p = "a";
#else
constexpr auto p = u8"a";
#endif
