#include "side.h"
// WP-035, digest-kinds §7 `_` `converting-constructor-against-conversion-function`.
struct T;
struct U { int i;
#ifdef B
  operator T() const;
#endif
};
struct T { int j;
#ifndef B
  T(const U &);
#endif
};
U u;
T v = u;
