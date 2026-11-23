#include "side.h"
// std: c++17

// digest-kinds §11.2 rule 4, `throw()`: as `noexcept(false)`, the written
// specification is the resolved unit's on both sides.
struct S { ~S() throw(); };
#ifdef B
bool h() { return noexcept(((S *)0)->~S()); }
#endif
