#include "side.h"
// std: c++26
// §5.7 ColdDeclData `contracts`: the predicate differs.
#ifdef B
void f(int x) pre(x > 1);
#else
void f(int x) pre(x > 0);
#endif
