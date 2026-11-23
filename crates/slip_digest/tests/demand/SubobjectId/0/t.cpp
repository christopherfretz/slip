#include "side.h"
// §10.6 SubobjectId `0` (demand, §11.7): a virtual call in constant evaluation: `final_overrider`, kiln_sema `oven_bridge.rs:174`; every unit stays equal.
struct A { virtual constexpr int f() const { return 1; } }; struct S : A { constexpr int f() const override { return 2; } };
#ifdef B
int y() { static constexpr S s; constexpr const A &a = s; constexpr int r = a.f(); return r; }
#endif
