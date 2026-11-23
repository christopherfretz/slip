// std: c++26
#include "side.h"
// §6 ExpansionForm `Destructuring`.
struct P { int x, y; };
constexpr P p{1, 2};
#ifdef B
void f() { template for (constexpr int e : p) ; }
#else
void f() { template for (constexpr int e : {1, 2}) ; }
#endif
