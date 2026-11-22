#include "side.h"
// digest §2.1: a use that instantiates a static data member of a class template does not list it; the order unit stays equal.
template <class T> struct A { static int s; };
template <class T> int A<T>::s = 1;
int a = 1;
#ifdef B
int y() { return A<int>::s; }
#endif
int b = 2;
