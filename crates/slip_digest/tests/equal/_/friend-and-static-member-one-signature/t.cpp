#include "side.h"
// digest-kinds §3.3: a static member function and a friend definition of
// one signature in a class template, and a namespace-scope declaration:
// no collision.
template <class T> struct A { static void g(long); friend void g(long) {} };
void g(long);
