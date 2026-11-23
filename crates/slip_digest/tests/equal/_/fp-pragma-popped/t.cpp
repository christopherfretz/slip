#include "side.h"
// WP-034 (§6): a body after the pragma is popped is equal to one that never saw it.
#ifdef B
#pragma float_control(precise, off, push)
#pragma float_control(pop)
void f() {}
#else
void f() {}
#endif
