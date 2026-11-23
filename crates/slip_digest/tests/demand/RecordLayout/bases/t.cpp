#include "side.h"
// §4.7 RecordLayout `bases` (demand, §11.7): `offsetof` of a base's field: `offsetof_base_steps`, kiln_sema `gnu.rs:507`; every unit stays equal.
struct A { char a; }; struct S : A { int i; };
#ifdef B
unsigned long y() { return __builtin_offsetof(S, a); }
#endif
