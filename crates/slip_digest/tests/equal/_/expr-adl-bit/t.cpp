#include "side.h"
// digest §6 _ `expr-adl-bit`.
namespace n1 { struct K {}; int f(K); }
n1::K k;
#ifdef B
int v = n1::f(k);
#else
int v = f(k);
#endif
