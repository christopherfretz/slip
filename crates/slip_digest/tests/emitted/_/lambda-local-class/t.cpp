#include "side.h"
// WP-155 fix round (B1): a local class inside a lambda's body; and a
// lambda that initialises a variable (the variable is marked for its
// initialiser too: a call is not plainly constant).
extern "C" int puts(const char *);
template <class U> auto s() {
  auto l = [] {
    struct L { __attribute__((constructor)) static void c() { puts("ran"); } };
    return 1;
  };
  return l();
}
int v = [] {
  struct M { __attribute__((used)) static void c() {} };
  return 1;
}();
#ifndef B
int x = sizeof(s<int>());
#endif
