#include "side.h"
// §4.7 RecordLayout `unadjusted_align` (demand, §11.7): `__builtin_source_location()`: the `__impl` object, kiln_sema `builtins.rs:3766`; every unit stays equal.
namespace std { struct source_location { struct __impl { const char *_M_file_name; const char *_M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
#ifdef B
const void *y() { return __builtin_source_location(); }
#endif
