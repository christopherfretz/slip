#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a anonymous namespace.
#ifdef B
namespace n { int v; }
#else
namespace { int v; }
#endif
