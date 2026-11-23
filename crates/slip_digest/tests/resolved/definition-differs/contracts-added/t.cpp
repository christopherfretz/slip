// std: c++26
#include "side.h"
// digest-kinds §11.9 (WP-045 review, N6): contracts and `0-has-contracts`
// of an instantiation are `definition`'s: its own unit does not differ.
template <class T> struct Q {
#ifdef B
  int g(int x) pre(x > 0) { return x; }
#else
  int g(int x) { return x; }
#endif
};
Q<int> q;
int h() { return q.g(1); }
