#include "side.h"
// §5.6 FriendTarget `Type.ty`: which type is befriended.
struct Y;
struct Z;
#ifdef B
struct X { friend Z; };
#else
struct X { friend Y; };
#endif
