#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 7): the type completed by
// the instantiated out-of-line definition is `definition`'s.
template <class T> struct S { static int a[]; };
#ifdef B
template <class T> int S<T>::a[4];
#else
template <class T> int S<T>::a[3];
#endif
int h() { return S<int>::a[0]; }
