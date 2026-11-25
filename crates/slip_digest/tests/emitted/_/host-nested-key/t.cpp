#include "side.h"
// WP-155 second fix round, rule 1: what the walk records for the unit
// being written survives a key computed after it (the key of M::g, a
// member of a second local class, is computed with the unit's state put
// aside). Only rule 1 marks a default member initialiser's unit.
extern "C" int puts(const char *);
template <class T> struct A {
  int x = [] {
    struct L { __attribute__((constructor)) static void c() { puts("ran"); } };
    struct M { static int g() { return 2; } };
    return M::g();
  }();
};
#ifndef B
int use() { A<int> a; return a.x; }
#endif
