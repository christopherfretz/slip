#include "side.h"
// §7 _: `offsetof-other-class-same-layout`: the fact differs.
struct A { int x; int y; }; struct Bq { int p; int q; };
#ifdef B
unsigned long v = __builtin_offsetof(Bq, q);
#else
unsigned long v = __builtin_offsetof(A, y);
#endif
