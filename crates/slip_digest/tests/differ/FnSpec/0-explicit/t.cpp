#include "side.h"
// §5.4 FnSpec `0-explicit`.
#ifdef B
struct C { explicit C(int); };
#else
struct C { C(int); };
#endif
