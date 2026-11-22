#include "side.h"
// §10.3 StringEncoding `Utf16`: `u"a"` against `U"a"` (reached only through a variable's value).
#ifdef B
constexpr auto p = U"a";
#else
constexpr auto p = u"a";
#endif
