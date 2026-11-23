#include "side.h"
// §4.7 RecordLayout `vptr` (demand, §11.7): a polymorphic object in constant evaluation: kiln_oven `ty.rs:1165`; every unit stays equal.
struct S { constexpr S() {} virtual constexpr int f() const { return 1; } int i = 2; };
#ifdef B
int y() { constexpr S s; return s.i; }
#endif
