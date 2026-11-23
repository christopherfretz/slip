#include "side.h"
// digest-kinds §11.1 property 2: the same initialiser evaluates to a
// different value; of x, only the resolved unit `value` differs.
#ifdef B
constexpr int k = 2;
#else
constexpr int k = 1;
#endif
constexpr int x = k + 1;
