#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 7): a static data member
// declared with an array of unknown bound, completed by the out-of-line
// definition the use instantiates: the own unit does not move.
template <class T> struct S { static int a[]; };
template <class T> int S<T>::a[3];
S<int> s;
#ifdef B
int use() { return S<int>::a[0]; }
#endif
