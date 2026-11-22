#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: a friend defined in the class that befriends it, against one defined at namespace scope.
#ifdef B
struct X { friend void f() {} };
#else
struct X { friend void f(); };
void f() {}
#endif
