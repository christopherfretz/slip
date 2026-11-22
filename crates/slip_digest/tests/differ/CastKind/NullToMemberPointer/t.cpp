#include "side.h"
// §7.2 CastKind `NullToMemberPointer`.
struct S { int m; float fm; };
S s;
#ifdef B
int S::*v = &S::m;
#else
int S::*v = 0;
#endif
