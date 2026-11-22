#include "side.h"
// digest §6 _ `expr-key-independence`.
// a key does not depend on the unit that asks (WP-024): two units name
// one entity whose key holds an expression
template<int N> void f(char(*)[N + 1]);
#ifdef B
auto v = &f<2>;
auto u = &f<1>;
#else
auto v = &f<1>;
auto u = &f<1>;
#endif
