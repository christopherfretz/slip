#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a enumerator.
#ifdef B
enum E { b };
#else
enum E { a };
#endif
