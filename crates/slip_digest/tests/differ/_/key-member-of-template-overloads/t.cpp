#include "side.h"
// digest §3.4, WP-039 item 1: members of a class template whose signatures meet in an instantiation: two keys.
template <class T> concept C = true;
template <class T> struct A { void f(T) requires C<T>; void f(int); };
A<int> *p;
#ifdef B
A<int> a;
#endif
