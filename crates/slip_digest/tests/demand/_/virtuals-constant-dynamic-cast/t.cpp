#include "side.h"
// WP-107 construct (demand, §11.7): a `dynamic_cast` in constant evaluation: the subobject graph, kiln_sema `oven_bridge.rs:178`; every unit stays equal.
struct A { virtual constexpr int f() const { return 1; } }; struct S : A { };
#ifdef B
bool y() { static constexpr S s; constexpr const A &a = s; constexpr bool r = dynamic_cast<const S *>(&a) != nullptr; return r; }
#endif
