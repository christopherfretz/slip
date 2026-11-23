#include "side.h"
// WP-034 (§6): a local const read by a later constant expression, its initializer evaluated elsewhere on B (in a function: a variable would change the order unit).
constexpr int k() { return 3; }
#ifdef B
int g() { return k(); }
#endif
void f() { const int n = k(); int a[n]; (void)a; }
