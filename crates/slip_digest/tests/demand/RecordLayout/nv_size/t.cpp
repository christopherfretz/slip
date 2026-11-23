#include "side.h"
// §4.7 RecordLayout `nv_size` (demand, §11.7): `__builtin_is_pointer_interconvertible_with_class`: kiln_sema `builtins.rs:2672`; every unit stays equal.
struct S { int i; char c; };
#ifdef B
bool y() { return __builtin_is_pointer_interconvertible_with_class(&S::c); }
#endif
