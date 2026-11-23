#include "side.h"
// digest-kinds §11.2 rule 4, `noexcept(expr)`: every destructor is of the
// deferrable class, so a written specification is in the resolved unit,
// present on both sides, and a use changes nothing.
struct S { ~S() noexcept(false); };
#ifdef B
bool h() { return noexcept(((S *)0)->~S()); }
#endif
