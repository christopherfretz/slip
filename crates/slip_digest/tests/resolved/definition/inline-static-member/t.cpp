#include "side.h"
// digest-kinds §11.9: the initialiser of an inline static data member of
// an implicit instantiation, and `0-inline`, are `definition`'s.
template <class T> struct A { static inline int s = 3; static constexpr int c = 4; };
A<int> a;
#ifdef B
int h() { return A<int>::s + A<int>::c; }
#endif
