#include "side.h"
// digest §2.1: a static data member defined outside its class is listed where it is defined.
struct S { static int s; };
#ifdef B
int a = 1;
int S::s = 2;
#else
int S::s = 2;
int a = 1;
#endif
