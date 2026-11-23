// std: c++20
#include "side.h"
// §10.2 OverloadedOperator `Delete`: `operatordelete` against `operatordelete[]` of one signature.
using Z = decltype(sizeof 0);
#ifdef B
struct S { static void operator delete[](void *); };
#else
struct S { static void operator delete(void *); };
#endif
