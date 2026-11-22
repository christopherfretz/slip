#include "side.h"
// WP-035, digest-kinds §7 `ConstructInfo` `zeroing`: the same constructor; defaulted on its first declaration it is not user-provided, so value-initialisation zeroes first.
struct Z {
#ifdef B
  Z();
#else
  Z() = default;
#endif
  int i;
  virtual void f();
};
#ifdef B
Z::Z() = default;
#endif
Z v = Z();
