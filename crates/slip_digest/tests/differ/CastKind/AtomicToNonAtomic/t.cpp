#include "side.h"
// §7.2 CastKind `AtomicToNonAtomic`.
int a; _Atomic(int) at;
#ifdef B
int v = a;
#else
int v = at;
#endif
