#include "side.h"
// §7 ExprNode `DeclRefExpr.bits-names-closure-copy`.
// a copy capture is reached in a function body
#ifdef B
int f(int x) { return [&] { return x; }(); }
#else
int f(int x) { return [=] { return x; }(); }
#endif
