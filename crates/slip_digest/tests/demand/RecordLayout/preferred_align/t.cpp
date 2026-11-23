#include "side.h"
// §4.7 RecordLayout `preferred_align` (demand, §11.7): `offsetof` of a field: `field_bit_offset`, kiln_sema `gnu.rs:471`, `:528`; every unit stays equal.
struct S { char c; int i; };
#ifdef B
unsigned long y() { return __builtin_offsetof(S, i); }
#endif
