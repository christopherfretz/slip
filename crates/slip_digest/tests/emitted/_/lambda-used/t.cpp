#include "side.h"
// WP-155 fix round (B1): a lambda's call operator with `used`, if Kiln
// accepts the attribute there (spec/landed/verify/WP-155.md §1 says
// which).
template <class U> auto s() {
  auto l = []() __attribute__((used)) { return 2; };
  return l();
}
#ifndef B
int x = sizeof(s<int>());
#endif
