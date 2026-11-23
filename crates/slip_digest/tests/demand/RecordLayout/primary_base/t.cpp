#include "side.h"
// §4.7 RecordLayout `primary_base` (demand, §11.7): `__is_layout_compatible` of a class with a primary base: kiln_sema `traits/special.rs:405`; every unit stays equal.
struct A { virtual void f() {} int a; }; struct S : A { int i; };
#ifdef B
bool y() { return __is_layout_compatible(S, S); }
#endif
