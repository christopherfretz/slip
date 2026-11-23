#include "side.h"
// digest §14 (WP-045 review probe): a member class defined out of line.
template <class T> struct X { struct In; };
template <class T> struct X<T>::In { int a; };
X<int> x;
#ifdef B
int use() { X<int>::In i{}; return i.a; }
#endif
