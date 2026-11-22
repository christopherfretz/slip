#include "side.h"
// digest §2.2 (WP-032): a friend class or friend class template declared
// by a class template makes no namespace-scope declaration when the class
// is instantiated (Kiln builds only the `Friend`): nothing declared moves.
template <class T> struct A {
  friend struct F;
  template <class U> friend class G;
};
#ifdef B
void use() { A<int> a; }
#else

#endif
