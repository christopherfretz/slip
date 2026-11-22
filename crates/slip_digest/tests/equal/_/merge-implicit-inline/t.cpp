#include "side.h"
// §2.3, §5.4 FnSpec `0-implicit-inline`: a declaration after an inline one is implicitly inline in Kiln; the entity is inline either way.
#ifdef B
inline void f(); void f() {}
#else
void f(); inline void f() {}
#endif
