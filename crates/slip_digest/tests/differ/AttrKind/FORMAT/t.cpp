#include "side.h"
// §10.1 AttrKind: `FORMAT`: the attribute on one side only.
#ifdef B
__attribute__((format(printf, 1, 2))) void f(const char *, ...);
#else
void f(const char *, ...);
#endif
