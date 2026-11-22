#include "side.h"
// §5.4 FnSpec `0-storage-inherited`, §2.3: `static` on the first declaration only, or on both.
#ifdef B
static void f(); static void f() {}
#else
static void f(); void f() {}
#endif
