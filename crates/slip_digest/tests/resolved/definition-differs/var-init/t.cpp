#include "side.h"
// digest-kinds §11.9 row `VarData` `init` of a static data member.
template <class T> struct A { static int s; };
#ifdef B
template <class T> int A<T>::s = 2;
#else
template <class T> int A<T>::s = 1;
#endif
int h() { return A<int>::s; }
