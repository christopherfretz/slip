#include "side.h"
// digest §2.1: only declarations move; the definitions keep their order.
#ifdef B
int a = 1;
extern int b;
int b = 2;
#else
extern int b;
int a = 1;
int b = 2;
#endif
