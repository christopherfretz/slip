#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXRewrittenBinaryOperator`: the comparison rewritten through `<=>` against the call it stands for.
struct R { int r; };
struct X { int i; };
bool operator<(R, int);
bool operator>(R, int);
R operator<=>(X, X);
X a, b, c;
#ifdef B
bool v = operator<(a <=> b, 0);
#else
bool v = a < b;
#endif
