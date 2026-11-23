#include "side.h"
// §4.7 RecordLayout `nv_align` (demand, §11.7): the address of a packed member (`-Waddress-of-packed-member`): kiln_sema `packed.rs:93`, `:117`; every unit stays equal.
struct __attribute__((packed)) S { char c; int i; };
#ifdef B
int *y() { static S s; return &s.i; }
#endif
