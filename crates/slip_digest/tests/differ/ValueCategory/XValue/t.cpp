#include "side.h"
// §7.1 ValueCategory `XValue`.
int a = 1, b = 2;
#ifdef B
int v = static_cast<int&>(a);
#else
int v = static_cast<int&&>(a);
#endif
