#include "side.h"
// §5.6 FriendTarget `Decl`: a befriended function against a befriended class.
void f();
struct Y;
#ifdef B
struct X { friend void f(); };
#else
struct X { friend Y; };
#endif
