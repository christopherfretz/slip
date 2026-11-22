#include "side.h"
// §7 ExprNode `CXXThisExpr.aux-implicit`. This contradicts WP-031's must-equal `this->m` against `m`: the row is `in`.
struct Th { int m; int m2;
#ifdef B
int n = m; };
#else
int n = this->m; };
#endif
