#include "side.h"
// §4.3 TypeView `Record`.
struct R1 {}; struct R2 {};
#ifdef B
int v;
#else
R1 v;
#endif
