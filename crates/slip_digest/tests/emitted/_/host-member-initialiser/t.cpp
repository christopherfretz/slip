#include "side.h"
// WP-155 second fix round (rule 1): a lambda in a default member
// initialiser of a class template, holding a local class whose member
// has `constructor` (C) or `used` (U). clang emits A<int>'s copy of the
// member and runs it before main; the unit whose stream writes the
// initialiser of the field the instantiation made is marked, in side A,
// which instantiates it, only.
extern "C" int puts(const char *);
template <class T> struct A {
  int x = [] { struct L { __attribute__((constructor)) static void c() { puts("ran"); } }; return 1; }();
};
template <class T> struct U {
  int x = [] { struct L { __attribute__((used)) static void c() {} }; return 1; }();
};
#ifndef B
int use() { A<int> a; U<int> u; return a.x + u.x; }
#endif
