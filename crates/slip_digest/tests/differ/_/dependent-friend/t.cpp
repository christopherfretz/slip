#include "side.h"
// digest §10: a friend of a class template declared in it is a unit (no unit
// vanishes), keyed by its signature.
template <class T> struct A {
#ifdef B
  friend void f(A*, long);
#else
  friend void f(A*, int);
#endif
};
