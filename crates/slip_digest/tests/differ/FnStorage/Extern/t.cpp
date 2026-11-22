#include "side.h"
// §5.4 FnStorage `Extern`: kept for an inline function.
#ifdef B
extern inline void f();
#else
inline void f();
#endif
