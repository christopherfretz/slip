#include "side.h"
// digest-kinds §11.9: the use of a static data member makes Kiln
// instantiate its out-of-line definition, a new declaration at namespace
// scope; it is `definition`'s, and the own unit does not read it.
template <class T> struct A { static int s; };
template <class T> int A<T>::s = 1;
A<int> a;
#ifdef B
int h() { return A<int>::s; }
#endif
