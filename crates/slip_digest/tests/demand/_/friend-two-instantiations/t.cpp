#include "side.h"
// digest §2.2 (WP-032 rules 1-3): two instantiations of one class
// template, each declaring the friends again: `k` (declared at namespace
// scope too), `j` (declared only by the instantiations: one derived unit),
// and `h` (one entity per specialisation).
template <class T> struct A {
  friend void k(int);
  friend void j(long);
  friend void h(A) {}
};
void k(int);
#ifdef B
A<int> a;
A<long> b;
#else

#endif
