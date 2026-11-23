#include "side.h"
// §10.6 CmpCategoryInfo `kind` (demand, §11.7): a built-in `<=>` on integers (`strong_ordering`): kiln_sema `class/compare/mod.rs:378`; every unit stays equal.
#include "compare.h"
#ifdef B
auto y() { return 1 <=> 2; }
#endif
