#include "side.h"
// §10.6 CmpCategoryInfo `field` (demand, §11.7): a defaulted `<=>` of a class with no subobject (`strong_ordering::equal`): kiln_sema `class/compare/synth.rs:214`, `:217`; every unit stays equal.
#include "compare.h"
#ifdef B
auto y() { struct X { auto operator<=>(const X &) const = default; }; return X{} <=> X{}; }
#endif
