#include "side.h"
// §5.3 FriendKind `Undeclared`.
#ifdef B
struct C { friend void f(); }; void f();
#else
struct C { friend void f(); };
#endif
