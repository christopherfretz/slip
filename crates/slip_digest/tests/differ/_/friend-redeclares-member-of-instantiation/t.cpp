#include "side.h"
// WP-045 (mutation M6): a friend declaration instantiated in A<int>
// redeclares C<int>::m; its lexical context is a class, so it is not an
// out-of-line instantiated definition and C<int>::m's own unit keeps it.
template <class T> struct C { void m(); };
template <class T> struct A {
#ifdef B
  friend void C<T>::m();
#endif
};
C<int> c;
A<int> a;
