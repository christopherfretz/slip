#include "side.h"
// digest-kinds §11.3, `NotConstant`: the value is absent, the unit is
// there.
int g();
const int n = g();
#ifdef B
int h() { return n; }
#endif
