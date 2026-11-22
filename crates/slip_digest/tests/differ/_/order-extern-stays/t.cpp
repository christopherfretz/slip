#include "side.h"
// digest §2.1: a declaration that is not a definition does not count; the definitions swap.
extern int a;
#ifdef B
int a = 1;
int b = 2;
#else
int b = 2;
int a = 1;
#endif
