#include "side.h"
// §4.7 RecordLayout `vbases` (demand, §11.7): `offsetof` in a class with a virtual base (conditionally supported): kiln_sema `gnu.rs:528`; every unit stays equal.
struct A { char a; }; struct S : virtual A { int i; };
#ifdef B
unsigned long y() { return __builtin_offsetof(S, i); }
#endif
