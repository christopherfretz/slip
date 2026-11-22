#include "side.h"
// digest §2.2 (WP-032 rule 2): `h(H<int>)` is declared only by the
// instantiation of `H<int>`, outside the class: a derived unit.
template <class T> struct H { friend void h(H) {} };
#ifdef B
H<int> hi;
void use() { h(hi); }
#else

#endif
