#include "side.h"
// WP-155 second fix round (rule 1): a lambda in the initialiser of a
// variable template, holding a local class whose member has
// `constructor` (vt) or `used` (vu).
extern "C" int puts(const char *);
template <class T> constexpr int vt = [] { struct L { __attribute__((constructor)) static void c() { puts("ran"); } }; return 1; }();
template <class T> constexpr int vu = [] { struct L { __attribute__((used)) static void c() {} }; return 1; }();
#ifndef B
const int *use() { return (void)&vu<int>, &vt<int>; }
#endif
