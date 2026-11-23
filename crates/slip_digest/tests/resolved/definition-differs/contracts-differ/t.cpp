// std: c++26
#include "side.h"
// digest-kinds §11.9 (WP-045 review, N9): both instantiations have a
// contract, which differs: `definition` writes the contracts.
template <class T> struct Q {
#ifdef B
  int g(int x) pre(x > 1) { return x; }
#else
  int g(int x) pre(x > 0) { return x; }
#endif
};
Q<int> q;
int h() { return q.g(1); }
