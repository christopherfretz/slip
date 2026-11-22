#include "side.h"
// §7.2 StorageDuration `FullExpression`.
struct X { ~X(); int m; int m2; };
struct X2 { ~X2(); int m; };
struct Y { int m; };
X mk(); X mk2(); X2 mkx2(); Y my();
int id(const int &);
#ifdef B
const int &v = 1;
#else
int v = id(1);
#endif
