#include "side.h"
// digest-kinds §11.9 row `ty-type` of a function: the deduced return type
// differs; only `definition` changes.
template <class T> struct Q { auto g() {
#ifdef B
  return 1L;
#else
  return 1;
#endif
} };
Q<int> q;
long h() { return q.g(); }
