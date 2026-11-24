#include "side.h"
// WP-155 fix round (B1): the `used` form. A static local with `used` in an
// instantiation is not emitted by clang unless the instantiation is
// (the reviewer ran it): it marks nothing (`k`).
template <class U> auto s() {
  struct L { __attribute__((used)) static void c() {} };
  return 1;
}
template <class U> auto k() {
  __attribute__((used)) static int n = 1;
  return n;
}
#ifndef B
int x = sizeof(s<int>()) + sizeof(k<int>());
#endif
