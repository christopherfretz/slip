#include "side.h"
// digest §2.1: a block-scope static is not at namespace or class scope and is not listed.
int a = 1;
#ifdef B
void g() { static int s = 3; }
#endif
int b = 2;
