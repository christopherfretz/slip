#include "side.h"
// digest-kinds §11.9 row `VarSpec` `0-init-style`.
template <class T> struct A { static int s; };
#ifdef B
template <class T> int A<T>::s{1};
#else
template <class T> int A<T>::s = 1;
#endif
int h() { return A<int>::s; }
