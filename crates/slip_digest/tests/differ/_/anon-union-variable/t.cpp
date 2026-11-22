#include "side.h"
// digest-kinds §5.2: the object of an anonymous union at namespace scope is
// a variable named by its union; its members are keyed through it.
static union { int a; };
#ifdef B
int other;
#endif
