#include "side.h"
// `extern` on an inline function definition at namespace scope says
// nothing about linkage, but the written storage class is kept
// (§5.4 FnStorage `Extern`), as for the declaration
// (`differ/FnStorage/Extern`).
#ifdef B
extern inline void f() {}
#else
inline void f() {}
#endif
