#include "side.h"
// §5.4 VarStorage `Static`.
#ifdef B
static int x;
#else
extern int x;
#endif
