#include "side.h"
// digest-kinds §11.9 (WP-045 review F2): as member-class-attr-alignas-in-class,
// the member class defined outside its class template.
template <class T> struct X { struct In; };
template <class T> struct alignas(16) X<T>::In { T v; char c; };
X<int>::In* p;
#ifdef B
int u() { X<int>::In i{}; return i.v; }
#endif
