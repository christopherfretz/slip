#include "side.h"
// A constructor template of a class that is not a template: Kiln keeps its list outside the AST too (in_template). Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
struct P { int m; template<class U> P(U) : m(X) { } };
