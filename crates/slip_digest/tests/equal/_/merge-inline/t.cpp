#include "side.h"
// §2.3, §5.4 FnSpec `0-inline`: inline on any declaration.
#ifdef B
void f(); inline void f();
#else
inline void f(); void f();
#endif
