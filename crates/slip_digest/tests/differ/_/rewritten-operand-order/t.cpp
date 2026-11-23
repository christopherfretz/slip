#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-operand-order`: `b` is `out`; the semantic form `a` carries the order.
struct R { int r; };
struct X { int i; };
bool operator<(R, int);
bool operator>(R, int);
R operator<=>(X, X);
X a, b, c;
#ifdef B
bool v = b < a;
#else
bool v = a < b;
#endif
