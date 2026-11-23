#include "side.h"
// digest-kinds §11.2 rule 2: an inheriting constructor, declared and
// resolved on both sides by the call in `k`; a second use changes nothing.
struct B0 { B0(int) noexcept; };
struct D : B0 { using B0::B0; };
void k() { D d(1); }
#ifdef B
bool h() { return noexcept(D(2)); }
#endif
