// std: c++26
#include "side.h"
// digest §4.3 "Names a program reads" (c): a reflection of a local class
// can read its name (`identifier_of`); a reflection is indeterminate
// today (`ExprKind/CXXReflectExpr`, `ValueKind/Reflection`), and the
// package that writes reflections owes the local entity's name.
#include "meta.h"
int f() { struct X {}; constexpr bool same = ^^X == ^^X; return same; }
