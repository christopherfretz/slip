#include "side.h"
// §5.3 FriendKind `Declared`: written as None.
#ifdef B
void f(); struct C { friend void f(); };
#else
struct C { friend void f(); }; void f();
#endif
