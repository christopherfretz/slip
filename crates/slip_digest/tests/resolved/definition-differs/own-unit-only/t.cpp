#include "side.h"
// digest-kinds §11.9, §11.1: a fact of the declaration of a member of an
// implicit instantiation differs; its `definition`, which starts from a
// fresh unit state, does not.
template <class T> struct Q {
#ifdef B
  [[nodiscard]]
#endif
  int g() { return 1; }
};
Q<int> q;
int h() { return q.g(); }
