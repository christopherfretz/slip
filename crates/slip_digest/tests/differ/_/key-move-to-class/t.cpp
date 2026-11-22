#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a move to class.
#ifdef B
namespace n {} struct C { static int v; };
#else
namespace n { int v; } struct C {};
#endif
