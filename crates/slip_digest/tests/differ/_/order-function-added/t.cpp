#include "side.h"
// digest §2.1: a function added between two variable definitions leaves the order unit alone.
int a = 1;
#ifdef B
void f() {}
#endif
int b = 2;
