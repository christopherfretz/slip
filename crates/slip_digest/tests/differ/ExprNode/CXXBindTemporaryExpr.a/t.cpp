#include "side.h"
// §7 ExprNode `CXXBindTemporaryExpr.a`.
struct X { ~X(); int m; int m2; };
struct X2 { ~X2(); int m; };
struct Y { int m; };
X mk(); X mk2(); X2 mkx2(); Y my();
int id(const int &);
#ifdef B
int v = mk2().m;
#else
int v = mk().m;
#endif
