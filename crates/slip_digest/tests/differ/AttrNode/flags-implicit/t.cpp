#include "side.h"
// §10.1 AttrNode: `flags-implicit`: the `format` Kiln gives `printf` against the same one written.
#ifdef B
extern "C" int printf(const char *, ...) __attribute__((format(printf, 1, 2)));
#else
extern "C" int printf(const char *, ...);
#endif
