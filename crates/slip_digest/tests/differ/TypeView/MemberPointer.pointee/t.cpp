#include "side.h"
// §4.3 `MemberPointer.pointee`.
struct C {}; struct D {};
#ifdef B
char C::* v;
#else
int C::* v;
#endif
