// std: c++20
#include "side.h"
// §10.2 OverloadedOperator `New`: `operatornew` against `operatornew[]` of one signature.
using Z = decltype(sizeof 0);
#ifdef B
struct S { static void *operator new[](Z); };
#else
struct S { static void *operator new(Z); };
#endif
