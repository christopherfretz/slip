#include "side.h"
// digest-kinds §11.2: a function outside the class writes its own
// specification and has no resolved unit, on either side.
struct S { void f() noexcept; S(); };
#ifdef B
bool h() { return noexcept(S().f()); }
#endif
