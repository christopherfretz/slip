#include "side.h"
// §5.7 `lexical_ctx` of an entity no declaration defines (WP-033 review, item 8): a friend in A against a friend in A and in C.
struct A { friend void f(); };
#ifdef B
struct C { friend void f(); };
#endif
