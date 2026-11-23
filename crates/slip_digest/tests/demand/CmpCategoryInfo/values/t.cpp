#include "side.h"
// §10.6 CmpCategoryInfo `values` (demand, §11.7): a defaulted `<=>` declared `weak_ordering` over a member without `<=>`: kiln_sema `class/compare/default.rs:433`, the synthesized comparison `three_way.rs:37`, `:61`; every unit stays equal.
#include "compare.h"
struct M { bool operator==(const M &) const; bool operator<(const M &) const; };
#ifdef B
auto y() { struct X { M m; std::weak_ordering operator<=>(const X &) const = default; }; return X{} <=> X{}; }
#endif
