#include "side.h"
// §7 ExprKind `ExprWithCleanups`.
struct X { ~X(); int m; int m2; };
struct X2 { ~X2(); int m; };
struct Y { int m; };
X mk(); X mk2(); X2 mkx2(); Y my();
int id(const int &);
#ifdef B
int v = my().m;
#else
int v = id(1);
#endif
