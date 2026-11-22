#include "side.h"
// §7.2 CastKind `NonAtomicToAtomic`.
int a; _Atomic(int) at;
#ifdef B
_Atomic(int) v = at;
#else
_Atomic(int) v = a;
#endif
