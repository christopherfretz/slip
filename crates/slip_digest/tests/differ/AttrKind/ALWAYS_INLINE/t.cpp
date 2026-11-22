#include "side.h"
// §10.1 AttrKind: `ALWAYS_INLINE`: the attribute on one side only.
#ifdef B
__attribute__((always_inline)) inline void f();
#else
inline void f();
#endif
