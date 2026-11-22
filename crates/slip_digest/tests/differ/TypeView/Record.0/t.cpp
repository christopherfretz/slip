#include "side.h"
// §4.3 `Record.0`: which class.
struct R1 {}; struct R2 {};
#ifdef B
R2 v;
#else
R1 v;
#endif
