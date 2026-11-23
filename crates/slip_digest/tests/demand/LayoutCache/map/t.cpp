#include "side.h"
// §4.7 LayoutCache `map` (demand, §11.7): `__is_layout_compatible`: `ensure_record_layout`, kiln_sema `templ/builtins/traits/special.rs:391`, `:405`; every unit stays equal.
struct S { char c; int i; }; struct T { char d; int j; };
#ifdef B
bool y() { return __is_layout_compatible(S, T); }
#endif
