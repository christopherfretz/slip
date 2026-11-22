#include "side.h"
// digest §2.1: a namespace-scope anonymous union's object is listed (WP-033 review, item 3).
int h();
#ifdef B
int g = h(); static union { int a = h(); };
#else
static union { int a = h(); }; int g = h();
#endif
