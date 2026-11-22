#include "side.h"
// WP-035, digest-kinds §7 `_` `cast-written-functional-construct`: ColdExprData `cast_written-cast` on a functional cast that constructs.
struct S { S(int); };
using CS = const S;
#ifdef B
S v = S(1);
#else
S v = CS(1);
#endif
