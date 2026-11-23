#include "side.h"
// digest-kinds §11.2 (WP-046 review F1): a destructor's written and
// implicit specifications differ only in the resolved unit; whatever
// observes the specification resolves it on both sides, so the unit is
// on both and differs.
struct S {
#ifdef B
  ~S() noexcept(false);
#else
  ~S();
#endif
};
bool h(S &s) { return noexcept(s.~S()); }
