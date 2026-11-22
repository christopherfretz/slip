#include "side.h"
// digest §2.2, WP-032 rule 3: a friend function template declared only by
// `A<int>` (its head one level deep) and `O<int>::I<int>` (two levels):
// the unit, and the name it is shown by, are read from the canonical
// candidate, whichever instantiation comes first.
template <class T> struct A { template <class U> friend void g(U); };
template <class T> struct O { template <class V> struct I { template <class U> friend void g(U); }; };
#ifdef B
O<int>::I<int> o;
A<int> a;
#else
A<int> a;
O<int>::I<int> o;
#endif
