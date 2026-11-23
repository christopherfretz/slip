#include "side.h"
// WP-046 review S2: a member of a local class has no key, so no resolved
// unit holds its exception specification: the local function's stream
// does, as Kiln holds it (digest-kinds §11.2).
void k() {
#ifdef B
  struct L { ~L() noexcept(false) {} };
#else
  struct L { ~L() {} };
#endif
  L l;
}
