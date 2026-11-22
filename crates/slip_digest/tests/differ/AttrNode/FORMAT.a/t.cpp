#include "side.h"
// §10.1 AttrNode: `FORMAT.a`: the fact differs.
#ifdef B
__attribute__((format(scanf, 1, 2))) void f(const char *, ...);
#else
__attribute__((format(printf, 1, 2))) void f(const char *, ...);
#endif
