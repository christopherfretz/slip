#include "side.h"
// WP-035, digest-kinds §7 `_` `parens-against-braces`.
struct S { S(int); };
#ifdef B
S v{1};
#else
S v(1);
#endif
