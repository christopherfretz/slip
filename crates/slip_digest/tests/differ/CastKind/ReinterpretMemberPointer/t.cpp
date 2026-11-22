#include "side.h"
// §7.2 CastKind `ReinterpretMemberPointer`.
struct S { int m; float fm; };
S s;
int S::*pm; float S::*fpm;
#ifdef B
float S::*v = fpm;
#else
float S::*v = reinterpret_cast<float S::*>(pm);
#endif
