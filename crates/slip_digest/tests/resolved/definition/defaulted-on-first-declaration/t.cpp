#include "side.h"
// digest-kinds §11.9: a function explicitly defaulted on its first
// declaration is defined on odr-use, as an implicit member is.
struct S { S() = default; S(const S&) = default; int a; };
S s0;
#ifdef B
S h() { return s0; }
#endif
