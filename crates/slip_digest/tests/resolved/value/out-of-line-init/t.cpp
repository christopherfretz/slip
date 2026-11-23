#include "side.h"
// digest-kinds §11.3: a static constant initialised out of line.
struct S { static const int x; };
const int S::x = 1;
#ifdef B
int h() { int a[S::x]; return sizeof a; }
#endif
