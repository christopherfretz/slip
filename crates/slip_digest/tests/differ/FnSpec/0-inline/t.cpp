#include "side.h"
// §5.4 FnSpec `0-inline`.
#ifdef B
inline void f();
#else
void f();
#endif
