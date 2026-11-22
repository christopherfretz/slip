#include "side.h"
// digest §2.1: the order unit lists the namespace-scope variable definitions in order; each variable's unit is unchanged.
#ifdef B
int b = 2;
int a = 1;
#else
int a = 1;
int b = 2;
#endif
