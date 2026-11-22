#include "side.h"
// §7 ExprNode `bits-value-category`.
int a = 1, b = 2;
#ifdef B
int v = static_cast<int&>(a);
#else
int v = static_cast<int&&>(a);
#endif
