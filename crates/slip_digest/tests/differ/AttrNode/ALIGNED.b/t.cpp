#include "side.h"
// §10.1 AttrNode: `ALIGNED.b`: the fact differs.
#ifdef B
alignas(long) char v;
#else
alignas(int) char v;
#endif
