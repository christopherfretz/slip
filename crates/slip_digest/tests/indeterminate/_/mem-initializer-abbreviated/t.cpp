#include "side.h"
// An abbreviated constructor template: its invented head is open at the definition. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
struct P { int m; P(auto) : m(X) { } };
