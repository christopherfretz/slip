#include "side.h"
// digest §6 _ `expr-qualified-virtual-call`.
struct Bs { virtual int f(); };
Bs *pb;
#ifdef B
int v = pb->f();
#else
int v = pb->Bs::f();
#endif
