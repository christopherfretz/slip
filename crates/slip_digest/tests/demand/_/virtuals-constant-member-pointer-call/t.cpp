#include "side.h"
// WP-107 construct (demand, §11.7): a call through a pointer to a virtual member in constant evaluation: kiln_sema `oven_bridge.rs:174`; every unit stays equal.
struct A { virtual constexpr int f() const { return 1; } }; struct S : A { constexpr int f() const override { return 2; } };
#ifdef B
int y() { static constexpr S s; constexpr int (A::*p)() const = &A::f; constexpr int r = (s.*p)(); return r; }
#endif
