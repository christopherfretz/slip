#include "side.h"
// §4.3 TypeView `Atomic`.
#ifdef B
int v;
#else
_Atomic(int) v;
#endif
