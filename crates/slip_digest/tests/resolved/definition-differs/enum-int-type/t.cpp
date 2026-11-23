#include "side.h"
// digest-kinds §11.9 rows `EnumData` `int_type` and `flags-fixed`.
template <class T> struct X {
#ifdef B
  enum E : long { e0 };
#else
  enum E { e0 };
#endif
};
X<int> x;
int h() { return X<int>::e0; }
