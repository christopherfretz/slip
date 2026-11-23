#include "side.h"
// digest-kinds §11.9 (WP-045 review F2): as member-class-attr-packed-in-class,
// the member class defined outside its class template.
template <class T> struct X { struct In; };
template <class T> struct __attribute__((packed)) X<T>::In { T v; char c; };
X<int>::In* p;
#ifdef B
int u() { X<int>::In i{}; return i.v; }
#endif
