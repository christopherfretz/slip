#include "side.h"
// §7.1 ValueCategory `LValue`.
int a = 1, b = 2;
#ifdef B
int v = static_cast<int&&>(a);
#else
int v = static_cast<int&>(a);
#endif
