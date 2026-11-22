#include "side.h"
// §5.4 FnSpec `0-deleted`.
#ifdef B
void f() = delete;
#else
void f();
#endif
