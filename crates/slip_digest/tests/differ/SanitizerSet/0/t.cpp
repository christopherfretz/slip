#include "side.h"
// §10.1 SanitizerSet: `0`: one sanitizer more in the set.
#ifdef B
__attribute__((no_sanitize("address", "thread"))) void f();
#else
__attribute__((no_sanitize("address"))) void f();
#endif
