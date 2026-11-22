#include "side.h"
// §7 ExprNode `DeclRefExpr.bits-refers-to-enclosing`.
// a capture is reached in a function body
#ifdef B
int f(int x) { return [&] { int y = 0; return y; }(); }
#else
int f(int x) { return [&] { return x; }(); }
#endif
