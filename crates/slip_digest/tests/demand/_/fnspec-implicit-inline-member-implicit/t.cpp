#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 1): the same for a member
// of an implicit instantiation.
template <class T> struct X { int g(int a); };
template <class T> inline int X<T>::g(int a) { return a; }
X<int> x;
#ifdef B
int use() { return x.g(1); }
#endif
