#include "side.h"
// §5.4 FnStorage `Extern`: written with the tag of `None` unless the function is inline.
#ifdef B
extern void f();
#else
void f();
#endif
