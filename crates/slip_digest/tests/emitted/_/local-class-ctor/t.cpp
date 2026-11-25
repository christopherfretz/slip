#include "side.h"
// WP-155 fix round (B1): a member function of a local class with
// `constructor` is emitted with the function whose body declares it
// (clang emits n::S::s<int>()::L::c() and runs it before main): the
// definition unit of that function is marked, in side A, which
// instantiates it, only. A local class's member function with no such
// attribute marks nothing (`plain` below).
extern "C" int puts(const char *);
template <class U> auto s() {
  struct L { __attribute__((constructor)) static void c() { puts("ran"); } };
  return 1;
}
template <class U> auto plain() {
  struct M { static void c() { puts("ran"); } };
  return 1;
}
int f() {
  struct N { struct O { __attribute__((destructor)) static void d() {} }; };
  return 0;
}
#ifndef B
int x = sizeof(s<int>()) + sizeof(plain<int>());
#endif
