// std: c++26
#include "side.h"
// digest-kinds §11.9 (WP-045 second fix round, mutant P6): Kiln copies
// the annotations of a member class's pattern definition with its
// attributes (`Substituter::class_attrs`,
// kiln_sema/src/templ/subst/anon.rs:140): they are `definition`'s, and
// X<int>::In's own unit does not move.
template <class T> struct X { struct [[=1]] In { T v; }; };
X<int>::In* p;
#ifdef B
int u() { X<int>::In i{}; return i.v; }
#endif
