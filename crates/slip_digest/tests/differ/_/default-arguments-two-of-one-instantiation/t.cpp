#include "side.h"
// digest-kinds §11.4, digest §3.4 (WP-045 third look): an instantiated
// function with two default arguments has two `default_argument` units
// whose keys differ only by the parameter's index; the second changes.
template <class T> int f(T a = 1, T b =
#ifdef B
  3
#else
  2
#endif
  ) { return a + b; }
int u() { return f<int>(); }
