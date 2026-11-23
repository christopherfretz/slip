#include "side.h"
// digest §3.4, WP-039 item 1: a hidden friend of a class template against a namespace-scope function of the instantiated signature: two keys.
template <class T> struct A { friend void g(A, int) {} };
A<int> a;
#ifdef B
void g(A<int>, int);
#endif
