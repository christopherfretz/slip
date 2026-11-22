#include "side.h"
// digest-kinds §5.6, WP-032 rule 1: the language linkage and specifiers of
// a function the instantiation declares first are those of its written
// declaration.
template <class T> struct A { friend void c2(int); };
#ifdef B
void use() { A<int> a; }
#endif
extern "C++" { void c2(int); }
