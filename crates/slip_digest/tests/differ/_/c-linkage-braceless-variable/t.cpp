#include "side.h"
// [dcl.link]/8 (WP-033 review, item 10): `extern "C" int x;` declares, `extern "C" { int x; }` defines; the variable's unit holds the difference without `aux-braceless`.
#ifdef B
extern "C" int x;
#else
extern "C" { int x; }
#endif
