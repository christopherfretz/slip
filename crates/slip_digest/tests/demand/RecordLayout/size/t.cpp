#include "side.h"
// §4.7 RecordLayout `size` (demand, §11.7): a class object in constant evaluation: the Oven's object type, kiln_oven `ty.rs:1165` through kiln_sema `oven_bridge.rs:125`; every unit stays equal.
struct S { char c; int i; };
#ifdef B
int y() { constexpr S s{1, 2}; return s.i; }
#endif
