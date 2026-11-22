#include "side.h"
// digest §6 _ `expr-key-independence`.
// a key does not depend on the unit that asks (WP-024): two units name
// one entity whose key holds an expression, first asked for by either
template<int N> void f(char(*)[N + 1]);
#ifdef B
void (*u)(char(*)[2]) = &f<1>;
void (*v)(char(*)[2]) = &f<1>;
#else
void (*v)(char(*)[2]) = &f<1>;
void (*u)(char(*)[2]) = &f<1>;
#endif
