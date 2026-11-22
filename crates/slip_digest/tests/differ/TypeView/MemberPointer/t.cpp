#include "side.h"
// §4.3 TypeView `MemberPointer`.
struct C {}; struct D {};
#ifdef B
int* v;
#else
int C::* v;
#endif
