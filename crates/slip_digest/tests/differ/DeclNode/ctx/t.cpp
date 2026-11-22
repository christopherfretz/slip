#include "side.h"
// §5.3 DeclNode `ctx` (keyed): a move to another namespace.
namespace a {} namespace b {}
#ifdef B
namespace b { int v; }
#else
namespace a { int v; }
#endif
