// std: c++20
#include "side.h"
// §10.2 OverloadedOperator `Exclaim`: `operator!` against `operator->` of one signature.
using Z = decltype(sizeof 0);
#ifdef B
struct S { S operator->(); };
#else
struct S { S operator!(); };
#endif
