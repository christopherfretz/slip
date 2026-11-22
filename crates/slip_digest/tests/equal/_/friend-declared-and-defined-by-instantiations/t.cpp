#include "side.h"
// digest §2.2, WP-032 rule 3: `j` is declared by `A<int>` and defined by
// `C<int>`; the unit is read from the canonical candidate alone, whichever
// instantiation comes first, and holds no definition of another.
template <class T> struct A { friend void j(long); };
template <class T> struct C { friend void j(long) {} };
#ifdef B
C<int> c;
A<int> a;
#else
A<int> a;
C<int> c;
#endif
