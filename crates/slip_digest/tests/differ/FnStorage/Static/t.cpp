#include "side.h"
// §5.4 FnStorage `Static`.
#ifdef B
static inline void f();
#else
inline void f();
#endif
