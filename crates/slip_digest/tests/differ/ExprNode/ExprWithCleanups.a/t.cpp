#include "side.h"
// §7 ExprNode `ExprWithCleanups.a`.
struct X { ~X(); int m; int m2; };
struct X2 { ~X2(); int m; };
struct Y { int m; };
X mk(); X mk2(); X2 mkx2(); Y my();
int id(const int &);
#ifdef B
int v = mk().m2;
#else
int v = mk().m;
#endif
