#include "side.h"
// WP-028 hazard (spec/landed/digest-kinds/WP-028.md "Hazards"): `&u.a` and `&u.b` differ, but Kiln stores a pointer with no member path, so both are one constant (same base, offset and designated type) and Kiln treats `A<&u.a>` as `A<&u.b>`.
template <auto V> struct A;
union U { int a; int b; } u;
#ifdef B
A<&u.b>* x;
#else
A<&u.a>* x;
#endif
