#include "side.h"
// §4.3 `MemberPointer.class`.
struct C {}; struct D {};
#ifdef B
int D::* v;
#else
int C::* v;
#endif
