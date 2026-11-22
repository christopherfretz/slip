#include "side.h"
// §5.5 BaseSpec `access`.
#ifdef B
struct P {}; struct C : protected P {};
#else
struct P {}; struct C : P {};
#endif
