#include "side.h"
// digest-kinds §11.3: an in-class static constant used in a constant
// expression on side B.
struct S { static const int x = 1; };
#ifdef B
int h() { int a[S::x]; return sizeof a; }
#endif
