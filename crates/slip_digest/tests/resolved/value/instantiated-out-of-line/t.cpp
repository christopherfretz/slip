#include "side.h"
// digest-kinds §11.3: a static data member of a class template
// initialised out of line, instantiated (and evaluated) on use.
template <class T> struct A { static const int v; };
template <class T> const int A<T>::v = sizeof(T);
A<int> a;
#ifdef B
int h() { int x[A<int>::v]; return sizeof x; }
#endif
