// std: c++17
#include "side.h"
// §5.3 DeclNode `LifetimeExtendedTemporary.data`: the temporary.
struct S { int a; };
#ifdef B
const S &r = S{2};
#else
const S &r = S{1};
#endif
