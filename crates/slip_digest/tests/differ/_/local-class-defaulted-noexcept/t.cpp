#include "side.h"
// WP-046 review S2: a constructor of a local class explicitly defaulted
// with and without `noexcept`; the member's constructor may throw.
struct M { M() noexcept(false); };
void k() {
#ifdef B
  struct L { M m; L() noexcept = default; };
#else
  struct L { M m; L() = default; };
#endif
  L l;
}
