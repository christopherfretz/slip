#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-operand-changed`: one operand changed alone, seen through the semantic form `a`.
struct R { int r; };
struct X { int i; };
bool operator<(R, int);
bool operator>(R, int);
R operator<=>(X, X);
X a, b, c;
#ifdef B
bool v = a < c;
#else
bool v = a < b;
#endif
