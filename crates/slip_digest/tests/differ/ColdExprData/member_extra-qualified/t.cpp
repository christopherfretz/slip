#include "side.h"
// §7.4 ColdExprData `member_extra-qualified`.
struct Bs { virtual int f(); };
Bs *pb;
#ifdef B
int v = pb->f();
#else
int v = pb->Bs::f();
#endif
