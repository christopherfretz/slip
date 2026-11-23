#include "side.h"
// digest-kinds §11.9 row `FunctionData` `body`: the body of a member
// function of an implicit instantiation differs; only `definition`
// changes (and the pattern).
template <class T> struct Q { int g() {
#ifdef B
  return 2;
#else
  return 1;
#endif
} };
Q<int> q;
int h() { return q.g(); }
