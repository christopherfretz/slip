#include "side.h"
// [dcl.link]/8 (WP-033 review, item 10): `extern "C" { extern int x; }` and `extern "C" int x;` both declare.
#ifdef B
extern "C" int x;
#else
extern "C" { extern int x; }
#endif
