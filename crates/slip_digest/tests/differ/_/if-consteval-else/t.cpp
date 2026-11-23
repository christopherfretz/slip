// std: c++23
#include "side.h"
// WP-034 (§6): `if consteval` has no condition; both branches are stored and written.
#ifdef B
constexpr int f() { if consteval { return 1; } else { return 3; } }
#else
constexpr int f() { if consteval { return 1; } else { return 2; } }
#endif
