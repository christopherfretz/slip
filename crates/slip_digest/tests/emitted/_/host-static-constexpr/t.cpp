#include "side.h"
// WP-155 second fix round (rule 1): a lambda in the initialiser of a
// class template's `static constexpr` data member, holding a local class
// whose member has `constructor` (C) or `used` (U).
extern "C" int puts(const char *);
template <class T> struct A {
  static constexpr int k = [] { struct L { __attribute__((constructor)) static void c() { puts("ran"); } }; return 1; }();
};
template <class T> struct U {
  static constexpr int k = [] { struct L { __attribute__((used)) static void c() {} }; return 1; }();
};
#ifndef B
const int *use() { return (void)&U<int>::k, &A<int>::k; }
#endif
