#include "side.h"
// §5.6 FriendTarget `Type`: a befriended class against a befriended function.
void f();
struct Y;
#ifdef B
struct X { friend Y; };
#else
struct X { friend void f(); };
#endif
