#include "side.h"
// §5.5 BaseSpec `ty`.
#ifdef B
struct P {}; struct Q {}; struct C : Q {};
#else
struct P {}; struct Q {}; struct C : P {};
#endif
