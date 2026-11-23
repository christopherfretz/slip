// std: c++20
#include "side.h"
// §10.2 OverloadedOperator `MinusMinus`: `operator--` against `operatorco_await` of one signature.
using Z = decltype(sizeof 0);
#ifdef B
struct S { S operator co_await(); };
#else
struct S { S operator--(); };
#endif
