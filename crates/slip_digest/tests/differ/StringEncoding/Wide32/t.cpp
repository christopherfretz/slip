#include "side.h"
// §10.3 StringEncoding `Wide32`: `L"a"` against `U"a"` (reached only through a variable's value).
#ifdef B
constexpr auto p = U"a";
#else
constexpr auto p = L"a";
#endif
