#include "side.h"
// §4.3 Quals `0-volatile`.
#ifdef B
int v;
#else
volatile int v;
#endif
