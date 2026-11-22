#include "side.h"
// §7.2 CastKind `MemberPointerToBoolean`.
struct S { int m; float fm; };
S s;
int S::*pm; int a;
#ifdef B
bool v = a;
#else
bool v = pm;
#endif
