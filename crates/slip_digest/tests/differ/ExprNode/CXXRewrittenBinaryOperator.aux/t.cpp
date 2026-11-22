#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXRewrittenBinaryOperator.aux`.
struct R { int r; };
struct X { int i; };
bool operator<(R, int);
bool operator>(R, int);
R operator<=>(X, X);
X a, b, c;
#ifdef B
bool v = a > b;
#else
bool v = a < b;
#endif
