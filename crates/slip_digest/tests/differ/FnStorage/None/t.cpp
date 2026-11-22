#include "side.h"
// §5.4 FnStorage `None`.
#ifdef B
void f();
#else
static void f();
#endif
