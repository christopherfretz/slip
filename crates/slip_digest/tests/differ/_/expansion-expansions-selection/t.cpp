// std: c++26
#include "side.h"
// WP-034 (§6): ExpansionData `expansions`: the second expansion selects another `g`; the pattern is the same.
#ifdef B
struct X {}; struct Y : X {}; void g(X);
void f() { template for (auto e : {X{}, Y{}}) g(e); }
#else
struct X {}; struct Y : X {}; void g(X); void g(Y);
void f() { template for (auto e : {X{}, Y{}}) g(e); }
#endif
