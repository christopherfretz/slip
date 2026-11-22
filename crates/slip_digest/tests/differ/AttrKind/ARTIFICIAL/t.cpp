#include "side.h"
// §10.1 AttrKind: `ARTIFICIAL`: the attribute on one side only.
#ifdef B
__attribute__((artificial)) inline void f() {};
#else
inline void f() {};
#endif
