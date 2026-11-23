#include "side.h"
// digest-kinds §11.3: the in-class static constant defined out of line.
struct S { static const int x = 1; };
const int S::x;
#ifdef B
int h() { int a[S::x]; return sizeof a; }
#endif
