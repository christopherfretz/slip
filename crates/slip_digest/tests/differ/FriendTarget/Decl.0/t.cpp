#include "side.h"
// §5.6 FriendTarget `Decl.0`: which function is befriended.
void f();
void g();
#ifdef B
struct X { friend void g(); };
#else
struct X { friend void f(); };
#endif
