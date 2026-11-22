#include "side.h"
// §5.3 FriendKind `None`.
#ifdef B
struct C { friend void f(); };
#else
void f(); struct C { friend void f(); };
#endif
