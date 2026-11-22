#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a constructor.
#ifdef B
struct C { C(long); };
#else
struct C { C(int); };
#endif
