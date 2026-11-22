#include "side.h"
// §10.1 AttrKind: `GNU_INLINE`: the attribute on one side only.
#ifdef B
__attribute__((gnu_inline)) extern inline void f() {};
#else
extern inline void f() {};
#endif
