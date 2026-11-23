#include "side.h"
// A local class in a generic lambda's body (the call operator is a template). Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
auto g = [](auto) { struct L { int m; L() : m(X) { } }; return 0; };
