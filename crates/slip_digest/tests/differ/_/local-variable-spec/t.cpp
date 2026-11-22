#include "side.h"
// §2.4, §5.1: a local variable writes its ordinal and stream; its spec is in it.
#ifdef B
void g() { static int x; }
#else
void g() { int x; }
#endif
