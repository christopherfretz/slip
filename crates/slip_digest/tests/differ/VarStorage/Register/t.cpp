#include "side.h"
// §5.4 VarStorage `Register`: a GNU register variable against `static`.
#ifdef B
static long v asm("rsp");
#else
register long v asm("rsp");
#endif
