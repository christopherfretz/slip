#include "side.h"
// digest §6 _ `expr-key-independence`.
// a key does not depend on the unit that asks (WP-024): two units name
// one entity whose key holds an expression, first asked for by either
// (two functions in either order: the order unit lists variables only).
template<int N> void f(char(*)[N + 1]);
#ifdef B
void u() { void (*p)(char(*)[2]) = &f<1>; (void)p; }
void v() { void (*p)(char(*)[2]) = &f<1>; (void)p; }
#else
void v() { void (*p)(char(*)[2]) = &f<1>; (void)p; }
void u() { void (*p)(char(*)[2]) = &f<1>; (void)p; }
#endif
