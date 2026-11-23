#include "side.h"
// As mem-initializer-constructor-template, with a specialisation defined: P::P<int>(int) is determinate. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
struct P { int m; template<class U> P(U) : m(X) { } };
P p(1);
