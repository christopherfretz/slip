#include "side.h"
// digest-kinds §11.9 row `ColdDeclData` `friends`.
void y();
void z();
template <class T> struct X {
  friend void y();
#ifdef B
  friend void z();
#endif
  int a;
};
X<int> x;
