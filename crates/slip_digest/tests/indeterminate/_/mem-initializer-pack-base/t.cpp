#include "side.h"
// A base initialised by a pack expansion. Cause kiln/pattern-mem-initializers (spec/landed/digest/WP-124.md).
#ifdef B
#define X 2
#else
#define X 1
#endif
struct A1 { A1(int); };
template<class... Ts> struct V : Ts... { V() : Ts(X)... { } };
