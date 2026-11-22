#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a namespace alias.
#ifdef B
namespace n {} namespace y = n;
#else
namespace n {} namespace x = n;
#endif
