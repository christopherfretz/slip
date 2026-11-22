#include "side.h"
// §4.3 TypeView `Elaborated`: sugar.
struct S {};
#ifdef B
struct S v;
#else
S v;
#endif
