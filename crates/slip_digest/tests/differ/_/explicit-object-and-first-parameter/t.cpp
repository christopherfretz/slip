// std: c++26
#include "side.h"
// digest-kinds rule §3.3 `function`: `f(this S&)` against `f(S&)`.
struct S {
#ifdef B
void f(this S&); void f(S&);
#else
void f(S&);
#endif
};
