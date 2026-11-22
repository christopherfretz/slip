#include "side.h"
// digest §6 _ `expr-several-candidates`.
namespace n1 { int f(int); }
namespace n2 { int f(double); }
using n1::f; using n2::f;
#ifdef B
int v = n1::f(1);
#else
int v = f(1);
#endif
