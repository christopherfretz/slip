#include "side.h"
// digest §2.2 (WP-032 rule 3): the friends two specialisations declare do
// not depend on which specialisation is instantiated first: `k` (declared
// at namespace scope too), `j` (declared only by the instantiations, one
// entity) and `h` (one entity per specialisation).
template <class T> struct A {
  friend void k(int);
  friend void j(long);
  friend void h(A) {}
};
void k(int);
#ifdef B
A<long> b;
A<int> a;
#else
A<int> a;
A<long> b;
#endif
