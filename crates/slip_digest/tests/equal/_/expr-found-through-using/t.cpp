#include "side.h"
// digest §6 _ `expr-found-through-using`.
namespace ns { int x; int f(int); }
using ns::x; using ns::f;
#ifdef B
int v = ns::x + ns::f(1);
#else
int v = x + f(1);
#endif
