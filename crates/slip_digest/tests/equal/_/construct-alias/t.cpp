#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-alias`: the class named through an alias.
struct S { S(int); };
using A = S;
#ifdef B
S v = A(1);
#else
S v = S(1);
#endif
