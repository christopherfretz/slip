#include "side.h"
// §5.7 `lexical_ctx` of an entity no declaration defines (WP-033 review, item 8): `ref*sorted` of the classes that befriend it, whichever is declared first.
#ifdef B
struct C { friend void f(); }; struct A { friend void f(); };
#else
struct A { friend void f(); }; struct C { friend void f(); };
#endif
