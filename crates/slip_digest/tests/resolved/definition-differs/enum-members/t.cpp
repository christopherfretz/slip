#include "side.h"
// digest-kinds §11.9 row `EnumData` `members`.
template <class T> struct X {
#ifdef B
  enum E { e0, e1 };
#else
  enum E { e0 };
#endif
};
X<int> x;
int h() { return X<int>::e0; }
