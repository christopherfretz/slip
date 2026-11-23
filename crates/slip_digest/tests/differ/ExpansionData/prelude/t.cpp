// std: c++26
#include "side.h"
// §6 ExpansionData `prelude`: the structured binding of another object.
struct P { int x, y; };
constexpr P p{1, 2};
constexpr P q{1, 2};
#ifdef B
void f() { template for (constexpr int e : q) ; }
#else
void f() { template for (constexpr int e : p) ; }
#endif
