#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a using declaration.
#ifdef B
namespace a { int x; int y; } using a::y;
#else
namespace a { int x; int y; } using a::x;
#endif
