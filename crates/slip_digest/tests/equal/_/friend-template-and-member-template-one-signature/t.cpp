#include "side.h"
// digest-kinds §3.3: a member function template and a friend function
// template of one signature in a class template; the function template
// inherits the befriended discriminator.
template <class T> struct A { template <class U> void g(U); template <class U> friend void g(U); };
