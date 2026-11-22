#include "side.h"
// §10.3 StringEncoding `Utf32`: `U"a"` against `u"a"` (reached only through a variable's value).
#ifdef B
constexpr auto p = u"a";
#else
constexpr auto p = U"a";
#endif
