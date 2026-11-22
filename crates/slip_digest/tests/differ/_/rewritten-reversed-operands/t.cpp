#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-reversed-operands`: the operands reversed against a candidate that takes them in order.
struct X { int i; };
#ifdef B
bool operator==(int, X);
#else
bool operator==(X, int);
#endif
X a;
bool v = 1 == a;
