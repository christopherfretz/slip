#include "side.h"
// digest-kinds §11.9 (WP-045 review F2): Kiln copies the attributes of a
// member class's pattern definition (__attribute__((packed))) onto the member of
// an implicit instantiation when it instantiates its definition
// (`Substituter::class_attrs`, `kiln_sema/src/templ/subst/anon.rs:140`):
// they are `definition`'s, and X<int>::In's own unit does not move.
template <class T> struct X { struct __attribute__((packed)) In { T v; char c; }; };
X<int>::In* p;
#ifdef B
int u() { X<int>::In i{}; return i.v; }
#endif
